import AVFoundation
import Foundation

/// Playback of a Speech Clip inside the app.
///
/// ADR 0007 puts this on the human entry point on purpose: `speech play`
/// under `--json` is a path resolver and validator that never makes a sound,
/// because the machine contract is shared with agents that must stay silent.
/// The verified path it returns is what gets played here.
///
/// The player is a protocol so the panel can be exercised without audio, and
/// so a failure to play is a testable state rather than something that only
/// shows up as silence in front of a user.
protocol SpeechAudioPlaying: AnyObject {
    var isPlaying: Bool { get }
    /// The clip currently loaded, if any.
    var currentClipID: String? { get }

    func play(clipID: String, url: URL) throws
    func stop()
}

enum SpeechAudioPlaybackError: LocalizedError, Equatable {
    case fileUnreadable(path: String)
    case engineFailed(details: String)

    var errorDescription: String? {
        switch self {
        case .fileUnreadable(let path):
            // ADR 0007: playback failure must not trigger a regeneration, so
            // this is reported as a terminal error rather than a retry prompt.
            return "音频文件无法读取：\(path)"
        case .engineFailed(let details):
            return "音频播放失败：\(details)"
        }
    }
}

/// `AVPlayer`-backed implementation.
///
/// The file the CLI returned is the one played, with no re-resolve: the CLI
/// has already checked the checksum, and re-deriving the location here would
/// reintroduce a second place that decides which audio a clip means.
final class SpeechAudioPlayer: NSObject, SpeechAudioPlaying, @unchecked Sendable {
    private var player: AVPlayer?
    private var observation: NSKeyValueObservation?
    private var endObserver: NSObjectProtocol?
    private var loadedClipID: String?

    /// Called on the main queue whenever playback state changes.
    var onStateChange: (() -> Void)?

    private(set) var isPlaying = false

    var currentClipID: String? { loadedClipID }

    func play(clipID: String, url: URL) throws {
        guard FileManager.default.isReadableFile(atPath: url.path) else {
            throw SpeechAudioPlaybackError.fileUnreadable(path: url.path)
        }

        // A new clip replaces the current one rather than queueing behind it:
        // the user picked this one, and stacking audio they did not ask for is
        // how a speech panel starts sounding broken.
        tearDown()

        let item = AVPlayerItem(url: url)
        let player = AVPlayer(playerItem: item)
        self.player = player

        observation = item.observe(\.status, options: [.new]) { [weak self] observed, _ in
            guard let self else { return }
            if observed.status == .failed {
                self.setIsPlaying(false)
            }
        }

        endObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime,
            object: item,
            queue: .main
        ) { [weak self] _ in
            self?.setIsPlaying(false)
        }

        player.play()
        loadedClipID = clipID
        setIsPlaying(true)
    }

    func stop() {
        player?.pause()
        tearDown()
        setIsPlaying(false)
    }

    private func tearDown() {
        if let endObserver {
            NotificationCenter.default.removeObserver(endObserver)
            self.endObserver = nil
        }
        observation?.invalidate()
        observation = nil
        player?.replaceCurrentItem(with: nil)
        player = nil
    }

    private func setIsPlaying(_ value: Bool) {
        guard isPlaying != value else { return }
        isPlaying = value
        if !value { loadedClipID = nil }
        let notify = onStateChange
        if Thread.isMainThread {
            notify?()
        } else {
            DispatchQueue.main.async { notify?() }
        }
    }
}
