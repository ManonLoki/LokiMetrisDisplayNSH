import Foundation

/// 已认证桌面授予的独立只读摘要凭据。字段只在 Swift 中搬运，校验与代次规则在 Rust。
struct WidgetRefreshTarget: Codable, Equatable {
    let version: Int
    let sourceId: String
    let deviceId: String
    let host: String
    let port: Int
    let key: String
    let generation: String

    func validate() throws {
        guard try LokiCore.call(["op": "target.valid", "target": LokiCore.object(self)]) as? Bool == true else {
            throw WidgetSnapshotError.invalid
        }
    }
}

/// 摘要刷新：Rust 生成固定地址与加密请求体、校验并解密响应；这里只执行一次 POST。
enum WidgetRefresh {
    enum Outcome: String { case success, newerCache, superseded, live, missingTarget, error }

    /// 系统扩展与 App 共用的共享目录刷新；结果写入前 Rust 再核对凭据代次。
    static func refresh(directory: URL) async -> Outcome {
        let gate = WidgetRefreshCommitGate()
        return await withTaskCancellationHandler {
            do {
                try Task.checkCancellation()
                let begin = try LokiCore.call(["op": "widget.begin", "dir": directory.path, "nowMs": Date().epochMs])
                guard let begin = begin as? [String: Any] else { return .error }
                if let skip = begin["skip"] as? String { return Outcome(rawValue: skip) ?? .error }
                guard let target = begin["target"], let request = begin["request"] as? [String: Any] else { return .error }
                let response = try await post(request)
                return try gate.commit {
                    try Task.checkCancellation()
                    let result = try LokiCore.call(["op": "widget.finish", "dir": directory.path, "target": target,
                        "request": request, "status": response.status, "mediaType": response.mediaType ?? NSNull(),
                        "body": response.body]) as? [String: Any]
                    return (result?["outcome"] as? String).flatMap(Outcome.init(rawValue:)) ?? .error
                }
            } catch { return .error }
        } onCancel: { gate.cancel() }
    }

    /// 不经共享目录的单次读取（watchOS 自有缓存）；返回 Rust 摘要记录。
    static func fetch(_ target: WidgetRefreshTarget) async throws -> [String: Any] {
        let targetObject = try LokiCore.object(target)
        guard let request = try LokiCore.call(["op": "widget.request", "target": targetObject,
            "nowMs": Date().epochMs]) as? [String: Any] else { throw WidgetSnapshotError.invalid }
        let response = try await post(request)
        guard let record = try LokiCore.call(["op": "widget.response", "target": targetObject, "request": request,
            "status": response.status, "mediaType": response.mediaType ?? NSNull(), "body": response.body]) as? [String: Any]
        else { throw WidgetSnapshotError.invalid }
        return record
    }

    private struct Response { let status: Int; let mediaType: String?; let body: String }

    /// 每次独立会话，不共享 Cookie、HTTP 认证、缓存或代理；拒绝重定向、限制响应大小。
    private static func post(_ request: [String: Any]) async throws -> Response {
        guard let url = (request["url"] as? String).flatMap(URL.init(string:)), let body = request["body"] as? String,
              let timeoutMs = (request["timeoutMs"] as? NSNumber)?.doubleValue,
              let limit = (request["maxResponseBytes"] as? NSNumber)?.intValue,
              let media = request["mediaType"] as? String else { throw WidgetSnapshotError.invalid }
        var http = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: timeoutMs / 1000)
        http.httpMethod = request["method"] as? String ?? "POST"
        http.setValue(media, forHTTPHeaderField: "Content-Type")
        http.setValue(media, forHTTPHeaderField: "Accept")
        http.httpShouldHandleCookies = false
        http.httpBody = Data(body.utf8)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = timeoutMs / 1000
        configuration.timeoutIntervalForResource = timeoutMs / 1000
        configuration.urlCache = nil
        configuration.urlCredentialStorage = nil
        configuration.httpCookieStorage = nil
        configuration.connectionProxyDictionary = [:]
        let (status, mediaType, data) = try await BoundedRequest(limit: limit, timeout: timeoutMs / 1000)
            .perform(http, configuration: configuration)
        return Response(status: status, mediaType: mediaType, body: String(decoding: data, as: UTF8.self))
    }
}

/// 流式收集且只恢复一次 continuation；被取消或超时立即终止底层连接。
private final class BoundedRequest: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    typealias Result = (Int, String?, Data)
    private let limit: Int
    private let timeout: TimeInterval
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Result, Error>?
    private var session: URLSession?
    private var deadline: DispatchWorkItem?
    private var finished = false
    private var status = 0
    private var mediaType: String?
    private var bytes = Data()

    init(limit: Int, timeout: TimeInterval) { self.limit = limit; self.timeout = timeout }

    func perform(_ request: URLRequest, configuration: URLSessionConfiguration) async throws -> Result {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                guard !finished else { lock.unlock(); continuation.resume(throwing: CancellationError()); return }
                self.continuation = continuation
                let queue = OperationQueue()
                queue.maxConcurrentOperationCount = 1
                let session = URLSession(configuration: configuration, delegate: self, delegateQueue: queue)
                let deadline = DispatchWorkItem { [weak self] in self?.finish(.failure(URLError(.timedOut))) }
                let task = session.dataTask(with: request)
                self.session = session; self.deadline = deadline
                lock.unlock()
                DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout, execute: deadline)
                task.resume()
            }
        } onCancel: { self.finish(.failure(CancellationError())) }
    }

    private func finish(_ result: Swift.Result<Result, Error>) {
        lock.lock()
        guard !finished else { lock.unlock(); return }
        finished = true
        let continuation = self.continuation, session = self.session, deadline = self.deadline
        self.continuation = nil; self.session = nil; self.deadline = nil
        lock.unlock()
        deadline?.cancel()
        session?.invalidateAndCancel()
        continuation?.resume(with: result)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                    completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        guard let response = response as? HTTPURLResponse, response.url == dataTask.originalRequest?.url,
              response.expectedContentLength <= Int64(limit) else {
            completionHandler(.cancel); finish(.failure(URLError(.badServerResponse))); return
        }
        lock.lock(); status = response.statusCode; mediaType = response.value(forHTTPHeaderField: "Content-Type"); lock.unlock()
        completionHandler(.allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        lock.lock()
        guard !finished, limit >= 0, bytes.count <= limit, data.count <= limit - bytes.count else {
            lock.unlock(); finish(.failure(URLError(.dataLengthExceedsMaximum))); return
        }
        bytes.append(data)
        lock.unlock()
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error { finish(.failure(error)); return }
        lock.lock(); let result = (status, mediaType, bytes); lock.unlock()
        finish(.success(result))
    }

    /// 拒绝任何重定向，防止摘要请求被引向其他主机。
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil); finish(.failure(URLError(.redirectToNonExistentLocation)))
    }

    /// 不接受系统凭据质询，认证只依赖请求内密钥。
    func urlSession(_ session: URLSession, task: URLSessionTask, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        completionHandler(.cancelAuthenticationChallenge, nil)
    }
}
