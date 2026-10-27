import CryptoKit
import Foundation
import Darwin

/// The only persistent input for the Watch UI. All state changes are serialized by this lock.
final class SnapshotCache: @unchecked Sendable {
    enum Origin { case context, file }
    private struct State: Codable {
        var manifest: CompanionManifest?
        var retiredStreams: [String]
    }
    private struct Pending: Codable {
        let streamId: String
        let revision: String
        let hash: String
    }
    private let root: URL
    private let lock = NSLock()
    private var state = State(manifest: nil, retiredStreams: [])
    private let files = FileManager.default
    private static let budget: UInt64 = 128 * 1024 * 1024

    init(root: URL) throws {
        self.root = root
        try files.createDirectory(at: root, withIntermediateDirectories: true)
        for name in ["assets", "pending", "incoming"] {
            try files.createDirectory(at: root.appendingPathComponent(name), withIntermediateDirectories: true)
        }
        if let data = try? Data(contentsOf: root.appendingPathComponent("state.json")), data.count <= 64 * 1024,
           let loaded = try? JSONDecoder().decode(State.self, from: data), loaded.retiredStreams.count <= 128,
           loaded.retiredStreams.allSatisfy({ UUID(uuidString: $0) != nil }) {
            do {
                try loaded.manifest?.validate()
                state = loaded
            } catch {
                // A corrupt local file never authorizes old display data; a new context can repair it.
            }
        }
        // Files moved from WCSession callbacks are not trusted state until reduced below.
        try removeChildren(in: root.appendingPathComponent("incoming"))
        try prune()
    }

    var manifest: CompanionManifest? { locked { state.manifest } }

    @discardableResult
    func receiveManifest(_ data: Data, origin: Origin) throws -> Bool {
        try locked {
            guard data.count <= 48 * 1024 else { throw CacheError.invalidManifest }
            let next = try JSONDecoder().decode(CompanionManifest.self, from: data)
            try next.validate()
            if let previous = state.manifest {
                if next.streamId == previous.streamId {
                    guard let new = next.sequence, let old = previous.sequence, new >= old else { throw CacheError.stale }
                    if new == old {
                        guard next == previous else { throw CacheError.conflictingRevision }
                        try installPending()
                        return false
                    }
                } else {
                    guard origin == .context, !state.retiredStreams.contains(next.streamId) else {
                        throw CacheError.unknownStream
                    }
                    guard state.retiredStreams.count < 128 else { throw CacheError.storageLimit }
                }
            } else if origin != .context {
                throw CacheError.unknownStream
            }
            var nextState = state
            if let previous = state.manifest, previous.streamId != next.streamId {
                nextState.retiredStreams.append(previous.streamId)
            }
            nextState.manifest = next
            // Publish the clear/new generation durably before touching old files.
            try save(nextState)
            state = nextState
            try prune()
            try installPending()
            return true
        }
    }

    /// Move the temporary WCSession URL before its delegate callback returns.
    func stageIncoming(_ temporary: URL) throws -> URL {
        try locked {
            let facts = try temporary.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
            guard facts.isRegularFile == true, facts.isSymbolicLink != true,
                  let size = facts.fileSize, (1...32 * 1024 * 1024).contains(size),
                  try diskUsage() + UInt64(size) <= Self.budget else { throw CacheError.storageLimit }
            let directory = root.appendingPathComponent("incoming")
            guard try files.contentsOfDirectory(atPath: directory.path).count < 12 else { throw CacheError.storageLimit }
            let destination = directory.appendingPathComponent(UUID().uuidString)
            try files.moveItem(at: temporary, to: destination)
            return destination
        }
    }

    @discardableResult
    func receiveAsset(_ staged: URL, streamId: String, revision: String, hash: String) throws -> Bool {
        try locked {
            defer { try? files.removeItem(at: staged) }
            guard UUID(uuidString: streamId) != nil, let sequence = canonicalUnsigned(revision), sequence > 0,
                  validHash(hash), !state.retiredStreams.contains(streamId) else { throw CacheError.invalidAsset }
            let facts = try inspect(staged)
            guard facts.hash == hash else { throw CacheError.invalidAsset }
            if let current = state.manifest, current.streamId == streamId, sequence <= current.sequence! {
                guard let descriptor = current.assets.first(where: { $0.hash == hash }),
                      descriptor.length == facts.length, descriptor.mime == facts.mime else { throw CacheError.invalidAsset }
                try moveVerified(staged, to: assetURL(hash))
                return true
            }
            let pending = root.appendingPathComponent("pending")
            let entries = try files.contentsOfDirectory(at: pending, includingPropertiesForKeys: nil)
                .filter { $0.pathExtension == "json" }
            guard entries.count < 12 else { throw CacheError.storageLimit }
            let key = "\(streamId)-\(revision)-\(hash)"
            let destination = pending.appendingPathComponent(key)
            try files.moveItem(at: staged, to: destination)
            try JSONEncoder().encode(Pending(streamId: streamId, revision: revision, hash: hash))
                .write(to: destination.appendingPathExtension("json"), options: .atomic)
            return false
        }
    }

    /// Return only a verified local image named by the current manifest.
    func imageURL(hash: String) -> URL? {
        locked {
            guard let descriptor = state.manifest?.assets.first(where: { $0.hash == hash }),
                  let facts = try? inspect(assetURL(hash)), facts.hash == hash,
                  facts.length == descriptor.length, facts.mime == descriptor.mime else { return nil }
            return assetURL(hash)
        }
    }

    private func installPending() throws {
        guard let current = state.manifest else { return }
        let directory = root.appendingPathComponent("pending")
        for sidecar in try files.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            where sidecar.pathExtension == "json" {
            guard let data = try? Data(contentsOf: sidecar), data.count < 1024,
                  let item = try? JSONDecoder().decode(Pending.self, from: data) else {
                try? files.removeItem(at: sidecar)
                continue
            }
            let image = sidecar.deletingPathExtension()
            if item.streamId == current.streamId && (canonicalUnsigned(item.revision) ?? UInt64.max) <= current.sequence! {
                if let descriptor = current.assets.first(where: { $0.hash == item.hash }),
                   let facts = try? inspect(image), facts.hash == descriptor.hash,
                   facts.length == descriptor.length, facts.mime == descriptor.mime {
                    try moveVerified(image, to: assetURL(item.hash))
                }
                try? files.removeItem(at: image)
                try? files.removeItem(at: sidecar)
            } else if state.retiredStreams.contains(item.streamId)
                || (item.streamId == current.streamId && (canonicalUnsigned(item.revision) ?? 0) < current.sequence!) {
                try? files.removeItem(at: image)
                try? files.removeItem(at: sidecar)
            }
        }
    }

    private func prune() throws {
        let keep = Set(state.manifest?.assets.map(\.hash) ?? [])
        for file in try files.contentsOfDirectory(at: root.appendingPathComponent("assets"), includingPropertiesForKeys: nil)
            where !keep.contains(file.lastPathComponent) {
            try? files.removeItem(at: file)
        }
        if state.manifest?.isClear == true { try removeChildren(in: root.appendingPathComponent("pending")) }
    }

    private func save(_ next: State) throws {
        try JSONEncoder().encode(next).write(to: root.appendingPathComponent("state.json"), options: .atomic)
    }

    private func assetURL(_ hash: String) -> URL { root.appendingPathComponent("assets/\(hash)") }

    private func moveVerified(_ source: URL, to destination: URL) throws {
        // Both URLs are inside this cache on one volume. rename atomically replaces any old copy.
        guard Darwin.rename(source.path, destination.path) == 0 else { throw CocoaError(.fileWriteUnknown) }
    }

    private func inspect(_ url: URL) throws -> (hash: String, mime: String, length: UInt64) {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, (1...32 * 1024 * 1024).contains(size) else { throw CacheError.invalidAsset }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let prefix = try handle.read(upToCount: 16) ?? Data()
        let mime: String
        if prefix.starts(with: [137,80,78,71,13,10,26,10]) { mime = "image/png" }
        else if prefix.starts(with: Data("GIF87a".utf8)) || prefix.starts(with: Data("GIF89a".utf8)) { mime = "image/gif" }
        else if prefix.starts(with: [255,216,255]) { mime = "image/jpeg" }
        else if prefix.starts(with: [66,77]) { mime = "image/bmp" }
        else if prefix.starts(with: Data("RIFF".utf8)), prefix.count >= 12,
                prefix[8..<12] == Data("WEBP".utf8) { mime = "image/webp" }
        else { throw CacheError.invalidAsset }
        var digest = SHA256()
        digest.update(data: prefix)
        var count = UInt64(prefix.count)
        while let bytes = try handle.read(upToCount: 64 * 1024), !bytes.isEmpty {
            count += UInt64(bytes.count)
            guard count <= 32 * 1024 * 1024 else { throw CacheError.invalidAsset }
            digest.update(data: bytes)
        }
        guard count == size else { throw CacheError.invalidAsset }
        return (digest.finalize().map { String(format: "%02x", $0) }.joined(), mime, count)
    }

    private func diskUsage() throws -> UInt64 {
        let keys: [URLResourceKey] = [.fileSizeKey, .isRegularFileKey]
        guard let enumerator = files.enumerator(at: root, includingPropertiesForKeys: keys) else { return 0 }
        return try enumerator.compactMap { $0 as? URL }.reduce(0) { total, url in
            let values = try url.resourceValues(forKeys: Set(keys))
            return total + UInt64(values.isRegularFile == true ? max(0, values.fileSize ?? 0) : 0)
        }
    }

    private func removeChildren(in directory: URL) throws {
        for file in try files.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) { try files.removeItem(at: file) }
    }

    private func locked<T>(_ operation: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try operation()
    }
}
