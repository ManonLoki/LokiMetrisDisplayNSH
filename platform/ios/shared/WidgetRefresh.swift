// 引入 Foundation
import Foundation

/// 摘要刷新：Rust 生成固定地址与加密请求体、校验并解密响应；这里只执行一次 POST。
// 摘要刷新：Rust 生成固定地址与加密请求体、校验并解密响应，这里只执行一次 POST
enum WidgetRefresh {
    // 刷新结果
    enum Outcome: String { case success, newerCache, superseded, live, missingTarget, error }

    /// 系统扩展与 App 共用的共享目录刷新；结果写入前 Rust 再核对凭据代次。
    // 系统扩展与 App 共用的共享目录刷新；写入结果前 Rust 会再核对凭据代次
    static func refresh(directory: URL) async -> Outcome {
        // 取消与提交共用的排序点
        let gate = WidgetRefreshCommitGate()
        // 支持取消地执行
        return await withTaskCancellationHandler {
            // 尝试执行
            do {
                // 检查是否被取消
                try Task.checkCancellation()
                // 向 Rust 询问是否需要刷新，以及请求内容
                let begin = try LokiCore.call(["op": "widget.begin", "dir": directory.path, "nowMs": Date().epochMs])
                // 开始响应须是字典
                guard let begin = begin as? [String: Any] else { return .error }
                // Rust 要求跳过（仍在租约内或没有凭据）
                if let skip = begin["skip"] as? String { return Outcome(rawValue: skip) ?? .error }
                // 取出凭据与请求
                guard let target = begin["target"], let request = begin["request"] as? [String: Any] else { return .error }
                // 执行一次网络请求
                let response = try await post(request)
                // 在排序点内把响应交给 Rust 校验并写入
                return try gate.commit {
                    // 检查是否被取消
                    try Task.checkCancellation()
                    // 结束刷新：交给展示核心
                    let result = try LokiCore.call(["op": "widget.finish", "dir": directory.path, "target": target,
                        // 请求、状态码与媒体类型
                        "request": request, "status": response.status, "mediaType": response.mediaType ?? NSNull(),
                        // 响应体
                        "body": response.body]) as? [String: Any]
                    // 取结果分类
                    return (result?["outcome"] as? String).flatMap(Outcome.init(rawValue:)) ?? .error
                }
            // 出错则返回错误结果
            } catch { return .error }
        // 取消时先关闭提交闸门
        } onCancel: { gate.cancel() }
    }

    // 响应：状态码、媒体类型与正文
    private struct Response { let status: Int; let mediaType: String?; let body: String }

    /// 每次独立会话，不共享 Cookie、HTTP 认证、缓存或代理；拒绝重定向、限制响应大小。
    // 每次使用独立会话，不共享 Cookie、HTTP 认证、缓存或代理；拒绝重定向并限制响应大小
    private static func post(_ request: [String: Any]) async throws -> Response {
        // 取出请求字段，缺任何一项都视为无效
        guard let url = (request["url"] as? String).flatMap(URL.init(string:)), let body = request["body"] as? String,
              // 超时毫秒数
              let timeoutMs = (request["timeoutMs"] as? NSNumber)?.doubleValue,
              // 响应大小上限
              let limit = (request["maxResponseBytes"] as? NSNumber)?.intValue,
              // 媒体类型，缺失则抛出
              let media = request["mediaType"] as? String else { throw WidgetSnapshotError.invalid }
        // 构造请求：忽略本地与远端缓存
        var http = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: timeoutMs / 1000)
        // 请求方法，默认 POST
        http.httpMethod = request["method"] as? String ?? "POST"
        // 内容类型
        http.setValue(media, forHTTPHeaderField: "Content-Type")
        // 接受类型
        http.setValue(media, forHTTPHeaderField: "Accept")
        // 不处理 Cookie
        http.httpShouldHandleCookies = false
        // 请求体
        http.httpBody = Data(body.utf8)
        // 临时会话：不保留任何存储
        let configuration = URLSessionConfiguration.ephemeral
        // 请求超时
        configuration.timeoutIntervalForRequest = timeoutMs / 1000
        // 资源超时
        configuration.timeoutIntervalForResource = timeoutMs / 1000
        // 不使用缓存
        configuration.urlCache = nil
        // 不使用凭据存储
        configuration.urlCredentialStorage = nil
        // 不使用 Cookie 存储
        configuration.httpCookieStorage = nil
        // 不经过代理
        configuration.connectionProxyDictionary = [:]
        // 带上限地发起请求
        let (status, mediaType, data) = try await BoundedRequest(limit: limit, timeout: timeoutMs / 1000)
            // 执行
            .perform(http, configuration: configuration)
        // 返回响应
        return Response(status: status, mediaType: mediaType, body: String(decoding: data, as: UTF8.self))
    }
}

/// 流式收集且只恢复一次 continuation；被取消或超时立即终止底层连接。
// 有界请求：流式收集，只恢复一次 continuation；取消或超时立即终止底层连接
private final class BoundedRequest: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    // 结果：状态码、媒体类型与数据
    typealias Result = (Int, String?, Data)
    // 响应大小上限
    private let limit: Int
    // 超时
    private let timeout: TimeInterval
    // 保护以下可变状态
    private let lock = NSLock()
    // 等待中的 continuation
    private var continuation: CheckedContinuation<Result, Error>?
    // 会话
    private var session: URLSession?
    // 超时任务
    private var deadline: DispatchWorkItem?
    // 是否已结束
    private var finished = false
    // 状态码
    private var status = 0
    // 媒体类型
    private var mediaType: String?
    // 已收到的字节
    private var bytes = Data()

    // 记录大小上限与超时
    init(limit: Int, timeout: TimeInterval) { self.limit = limit; self.timeout = timeout }

    // 发起请求并等待结果；任务被取消时立即结束
    func perform(_ request: URLRequest, configuration: URLSessionConfiguration) async throws -> Result {
        // 支持取消地执行
        try await withTaskCancellationHandler {
            // 等待 continuation
            try await withCheckedThrowingContinuation { continuation in
                // 加锁
                lock.lock()
                // 已结束（如启动前就被取消）
                guard !finished else { lock.unlock(); continuation.resume(throwing: CancellationError()); return }
                // 保存 continuation
                self.continuation = continuation
                // 代理回调在单线程队列上串行执行
                let queue = OperationQueue()
                // 最多并发一个操作
                queue.maxConcurrentOperationCount = 1
                // 创建会话
                let session = URLSession(configuration: configuration, delegate: self, delegateQueue: queue)
                // 到期则以超时结束
                let deadline = DispatchWorkItem { [weak self] in self?.finish(.failure(URLError(.timedOut))) }
                // 创建数据任务
                let task = session.dataTask(with: request)
                // 保存会话与期限
                self.session = session; self.deadline = deadline
                // 解锁
                lock.unlock()
                // 安排超时
                DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout, execute: deadline)
                // 启动任务
                task.resume()
            }
        // 取消时以取消错误结束
        } onCancel: { self.finish(.failure(CancellationError())) }
    }

    // 结束请求：只有第一次调用有效，取消超时、终止会话并恢复 continuation
    private func finish(_ result: Swift.Result<Result, Error>) {
        // 加锁
        lock.lock()
        // 已结束则解锁并返回
        guard !finished else { lock.unlock(); return }
        // 标记已结束
        finished = true
        // 取出 continuation、会话与期限
        let continuation = self.continuation, session = self.session, deadline = self.deadline
        // 清空引用
        self.continuation = nil; self.session = nil; self.deadline = nil
        // 解锁
        lock.unlock()
        // 取消期限任务
        deadline?.cancel()
        // 终止底层会话
        session?.invalidateAndCancel()
        // 恢复 continuation
        continuation?.resume(with: result)
    }

    // 收到响应头：地址必须与原请求一致，声明的长度不得超限，否则取消
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                    // 完成回调
                    completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        // 响应须是原请求地址的 HTTP 响应
        guard let response = response as? HTTPURLResponse, response.url == dataTask.originalRequest?.url,
              // 声明的长度不得超过上限
              response.expectedContentLength <= Int64(limit) else {
            // 否则取消并以错误响应结束
            completionHandler(.cancel); finish(.failure(URLError(.badServerResponse))); return
        }
        // 记录状态码和媒体类型
        lock.lock(); status = response.statusCode; mediaType = response.value(forHTTPHeaderField: "Content-Type"); lock.unlock()
        // 允许继续接收
        completionHandler(.allow)
    }

    // 收到数据块：累计不得超过上限，超限立即失败
    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        // 加锁
        lock.lock()
        // 未结束且累计长度在上限内
        guard !finished, limit >= 0, bytes.count <= limit, data.count <= limit - bytes.count else {
            // 超过大小上限：解锁并结束
            lock.unlock(); finish(.failure(URLError(.dataLengthExceedsMaximum))); return
        }
        // 追加数据
        bytes.append(data)
        // 解锁
        lock.unlock()
    }

    // 请求完成：有错误则失败，否则返回状态、类型与内容
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        // 有错误则以失败结束
        if let error { finish(.failure(error)); return }
        // 取出结果
        lock.lock(); let result = (status, mediaType, bytes); lock.unlock()
        // 以成功结束
        finish(.success(result))
    }

    /// 拒绝任何重定向，防止摘要请求被引向其他主机。
    // 拒绝任何重定向，防止摘要请求被引向其他主机
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    // 新请求（重定向）与完成回调
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        // 拒绝重定向并以错误结束
        completionHandler(nil); finish(.failure(URLError(.redirectToNonExistentLocation)))
    }

    /// 不接受系统凭据质询，认证只依赖请求内密钥。
    // 不接受系统凭据质询，认证只依赖请求内的密钥
    func urlSession(_ session: URLSession, task: URLSessionTask, didReceive challenge: URLAuthenticationChallenge,
                    // 质询的完成回调
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        // 取消认证质询
        completionHandler(.cancelAuthenticationChallenge, nil)
    }
}
