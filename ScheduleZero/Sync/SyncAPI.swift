import Foundation

/// Server answered with an error status.
struct HTTPError: Error {
    let code: Int
    let body: String
}

/// Tiny HTTP/JSON client for server/PROTOCOL.md.
struct SyncAPI {
    let baseURL: String
    let deviceId: String
    let deviceName: String
    var session: URLSession = SyncAPI.session

    static let session: URLSession = {
        let c = URLSessionConfiguration.ephemeral
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        c.urlCache = nil
        c.waitsForConnectivity = false
        c.timeoutIntervalForRequest = 60
        c.timeoutIntervalForResource = 90
        return URLSession(configuration: c)
    }()

    struct State {
        let rev: Int64
        /// nil when the server said "unchanged".
        let people: [Person: [Day: SyncedDay]]?
    }

    func authStatus(_ person: Person) async throws -> Bool {
        let json = try await request("GET", "/api/auth/status?person=\(person.rawValue)")
        return (json["hasPassword"] as? Bool) ?? false
    }

    /// Returns a token. Throws HTTPError 401 (wrong), 404 (no password), 409 (exists), 400 (too short).
    func authenticate(_ person: Person, password: String, setup: Bool) async throws -> String {
        let body: [String: Any] = [
            "person": person.rawValue,
            "password": password,
            "deviceId": deviceId,
            "deviceName": deviceName,
            "platform": "ios",
        ]
        let json = try await request("POST", setup ? "/api/auth/setup" : "/api/auth/login", body: body)
        guard let token = json["token"] as? String else { throw URLError(.cannotParseResponse) }
        return token
    }

    func state(token: String, since: Int64, waitSeconds: Int) async throws -> State {
        let json = try await request(
            "GET", "/api/state?since=\(since)&wait=\(waitSeconds)", token: token,
            timeout: TimeInterval(waitSeconds + 15)
        )
        return try Self.parseState(json)
    }

    @discardableResult
    func upload(token: String, days: [Day: SyncedDay]) async throws -> Int64 {
        var payload: [String: Any] = [:]
        for (d, day) in days { payload[d.iso] = Self.dayJSON(day) }
        let json = try await request("POST", "/api/schedule", token: token, body: ["days": payload])
        return Self.int64(json["rev"]) ?? -1
    }

    // MARK: JSON

    static func parseState(_ json: [String: Any]) throws -> State {
        guard let rev = int64(json["rev"]) else { throw URLError(.cannotParseResponse) }
        if (json["unchanged"] as? Bool) == true { return State(rev: rev, people: nil) }
        let people = json["people"] as? [String: Any] ?? [:]
        var map: [Person: [Day: SyncedDay]] = [:]
        for p in Person.allCases {
            var days: [Day: SyncedDay] = [:]
            for (key, value) in people[p.rawValue] as? [String: Any] ?? [:] {
                guard let d = Day(iso: key), let o = value as? [String: Any], let s = parseDay(o) else { continue }
                days[d] = s
            }
            map[p] = days
        }
        return State(rev: rev, people: map)
    }

    static func parseDay(_ o: [String: Any]) -> SyncedDay? {
        let updated = int64(o["updated"]) ?? 0
        switch o["type"] as? String {
        case "work":
            guard let s = int64(o["start"]), let e = int64(o["end"]) else { return nil }
            return SyncedDay(plan: .work(WorkShift(start: Int(s), end: Int(e))), updated: updated)
        case "off": return SyncedDay(plan: .off, updated: updated)
        case "none": return SyncedDay(plan: nil, updated: updated)
        default: return nil
        }
    }

    static func dayJSON(_ day: SyncedDay) -> [String: Any] {
        var o: [String: Any] = ["updated": day.updated]
        switch day.plan {
        case .work(let w): o["type"] = "work"; o["start"] = w.start; o["end"] = w.end
        case .off: o["type"] = "off"
        case nil: o["type"] = "none"
        }
        return o
    }

    static func int64(_ v: Any?) -> Int64? {
        if let n = v as? NSNumber { return n.int64Value }
        if let s = v as? String { return Int64(s) }
        return nil
    }

    // MARK: HTTP

    private func request(
        _ method: String, _ path: String, token: String? = nil, body: [String: Any]? = nil,
        timeout: TimeInterval = 15
    ) async throws -> [String: Any] {
        let trimmed = baseURL.hasSuffix("/") ? String(baseURL.dropLast()) : baseURL
        guard let url = URL(string: trimmed + path) else { throw URLError(.badURL) }
        var req = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: timeout)
        req.httpMethod = method
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue(deviceId, forHTTPHeaderField: "X-Device-Id")
        req.setValue(deviceName, forHTTPHeaderField: "X-Device-Name")
        req.setValue("ios", forHTTPHeaderField: "X-Platform")
        if let token { req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body {
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
            req.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await session.data(for: req)
        let code = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200...299).contains(code) else {
            throw HTTPError(code: code, body: String(data: data, encoding: .utf8) ?? "")
        }
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw URLError(.cannotParseResponse)
        }
        return json
    }
}
