import Foundation

/// Runs an action once input has been quiet for `delay`. Each `schedule`
/// call replaces the pending action, so work happens when the writer
/// pauses, never per keystroke.
@MainActor
public final class Debouncer {
    public let delay: Duration
    private var pending: Task<Void, Never>?

    public init(delay: Duration) {
        self.delay = delay
    }

    public func schedule(_ action: @escaping @MainActor () async -> Void) {
        pending?.cancel()
        pending = Task { [delay] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            await action()
        }
    }

    /// Runs the pending action now instead of waiting, if there is one.
    public func flush(_ action: @escaping @MainActor () async -> Void) async {
        guard pending != nil else { return }
        pending?.cancel()
        pending = nil
        await action()
    }

    public func cancel() {
        pending?.cancel()
        pending = nil
    }
}
