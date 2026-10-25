import Foundation

struct EmbeddedLaunchRequest: Decodable {
    let url: String
    let title: String
    let file_length: UInt64
    let demuxer_max_bytes: UInt64
    let cache_secs: UInt32?
    let audio_languages: [String]
    let subtitle_languages: [String]
    let audio_track: Int?
    let subtitle_track: Int?
    let subtitle_files: [String]
    let start_at: Double?
}

protocol FlixPlayerPresenter: AnyObject {
    func presentPlayer(with request: EmbeddedLaunchRequest)
    func dismissPlayerFromCore()
}

private func cLaunchCallback(payloadPtr: UnsafePointer<CChar>?) -> Int32 {
    guard let payloadPtr else { return -1 }
    let json = String(cString: payloadPtr)
    guard let data = json.data(using: .utf8),
          let request = try? JSONDecoder().decode(EmbeddedLaunchRequest.self, from: data) else {
        return -2
    }
    DispatchQueue.main.async {
        FlixBridge.shared.presenter?.presentPlayer(with: request)
    }
    return 0
}

private func cStopCallback() -> Int32 {
    DispatchQueue.main.async {
        FlixBridge.shared.presenter?.dismissPlayerFromCore()
    }
    return 0
}

final class FlixBridge {
    static let shared = FlixBridge()
    weak var presenter: FlixPlayerPresenter?

    private init() {}

    func startServer() -> URL? {
        flix_ios_register_player_callbacks(cLaunchCallback, cStopCallback)
        guard let rawPtr = flix_ios_start(0) else {
            return nil
        }
        defer { flix_ios_free_string(rawPtr) }
        let urlString = String(cString: rawPtr)
        return URL(string: urlString)
    }

    func stopServer() {
        flix_ios_clear_player_callbacks()
        flix_ios_stop()
    }

    func reportPlayerExit(success: Bool, exitCode: Int32) {
        flix_ios_report_player_exit(success, exitCode)
    }
}
