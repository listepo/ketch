// Where the ketch root is, resolved the way the CLI resolves it, so the app
// and the CLI share one state, one lock and one store.

import Foundation

enum KetchRoot {
    /// `KETCH_ROOT` when set and non-empty, else `~/.ketch`. An empty value
    /// counts as unset, as it does for the CLI: that is what a CI job writes
    /// when it means "no override".
    static func resolve(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL {
        if let value = environment["KETCH_ROOT"], !value.isEmpty {
            return URL(fileURLWithPath: (value as NSString).expandingTildeInPath, isDirectory: true)
        }
        return home.appending(path: ".ketch", directoryHint: .isDirectory)
    }

    /// The CLI's config file under `root`.
    static func configFile(in root: URL) -> URL {
        root.appending(path: "config.toml", directoryHint: .notDirectory)
    }
}
