import Foundation
import Testing
@testable import GrafKit

@Suite struct ParagraphTests {
    private let text = "First paragraph\ncontinues here.\n\nSecond one.\n\nThird." as NSString

    @Test func caretInsideAParagraphSelectsAllOfItsLines() {
        let range = Paragraphs.range(in: text, containing: 3)
        #expect(text.substring(with: range) == "First paragraph\ncontinues here.")
    }

    @Test func caretOnABlankLineSelectsNothing() {
        let blank = text.range(of: "\n\n").location + 1
        #expect(Paragraphs.range(in: text, containing: blank).length == 0)
    }

    @Test func caretAtTheEndBelongsToTheLastParagraph() {
        let range = Paragraphs.range(in: text, containing: text.length)
        #expect(text.substring(with: range) == "Third.")
    }

    @Test func caretAfterATrailingNewlineIsOnAnEmptyLine() {
        let withNewline = "Only line.\n" as NSString
        #expect(Paragraphs.range(in: withNewline, containing: withNewline.length).length == 0)
    }

    @Test func emptyTextHasAnEmptyParagraph() {
        #expect(Paragraphs.range(in: "" as NSString, containing: 0) == NSRange(location: 0, length: 0))
    }
}

@Suite struct SlugTests {
    @Test func namesBecomeLowercaseHyphenatedFolders() {
        #expect(folderSlug(for: "Reading and Recall!") == "reading-and-recall")
        #expect(folderSlug(for: "  Tésis   final  ") == "tesis-final")
        #expect(folderSlug(for: "???") == "untitled")
    }
}

@Suite struct RecentProjectsTests {
    private func store() -> RecentProjects {
        let suite = "graf.tests.\(UUID().uuidString)"
        return RecentProjects(defaults: UserDefaults(suiteName: suite)!)
    }

    @Test func recordingMovesAProjectToTheTop() {
        let recents = store()
        recents.record(RecentProject(path: "/a", name: "a"))
        recents.record(RecentProject(path: "/b", name: "b"))
        recents.record(RecentProject(path: "/a", name: "a", lastFile: "main.tex", caret: 42))
        #expect(recents.all.map(\.path) == ["/a", "/b"])
        #expect(recents.all.first?.caret == 42)
    }

    @Test func theListIsCapped() {
        let recents = store()
        for index in 0..<(RecentProjects.limit + 3) {
            recents.record(RecentProject(path: "/p\(index)", name: "p\(index)"))
        }
        #expect(recents.all.count == RecentProjects.limit)
    }
}

@MainActor
@Suite struct DebouncerTests {
    @Test func onlyTheLastScheduledActionRuns() async throws {
        let debouncer = Debouncer(delay: .milliseconds(30))
        var runs: [Int] = []
        for value in 1...3 {
            debouncer.schedule { runs.append(value) }
        }
        try await Task.sleep(for: .milliseconds(150))
        #expect(runs == [3])
    }

    @Test func flushRunsThePendingActionImmediately() async {
        let debouncer = Debouncer(delay: .seconds(10))
        var ran = false
        debouncer.schedule { }
        await debouncer.flush { ran = true }
        #expect(ran)
    }
}
