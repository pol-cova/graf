import Foundation

/// Where the writer was in a project when they last left it.
public struct RecentProject: Codable, Equatable, Sendable, Identifiable {
    public var path: String
    public var name: String
    /// The file that was open, relative to the project root.
    public var lastFile: String?
    /// UTF-16 caret offset in `lastFile`.
    public var caret: Int
    public var openedAt: Date

    public var id: String { path }

    public init(path: String, name: String, lastFile: String? = nil, caret: Int = 0, openedAt: Date = .now) {
        self.path = path
        self.name = name
        self.lastFile = lastFile
        self.caret = caret
        self.openedAt = openedAt
    }
}

/// Recently opened projects, newest first, stored in user defaults.
public struct RecentProjects {
    public static let limit = 8
    private let defaults: UserDefaults
    private let key = "graf.recentProjects"

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var all: [RecentProject] {
        guard let data = defaults.data(forKey: key),
              let projects = try? JSONDecoder().decode([RecentProject].self, from: data)
        else { return [] }
        return projects
    }

    /// Adds or refreshes `project` at the top of the list.
    public func record(_ project: RecentProject) {
        var projects = all.filter { $0.path != project.path }
        projects.insert(project, at: 0)
        save(Array(projects.prefix(Self.limit)))
    }

    public func remove(path: String) {
        save(all.filter { $0.path != path })
    }

    private func save(_ projects: [RecentProject]) {
        guard let data = try? JSONEncoder().encode(projects) else { return }
        defaults.set(data, forKey: key)
    }
}
