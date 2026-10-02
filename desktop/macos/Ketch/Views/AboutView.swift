// About: version, the triple licence and the repository.

import SwiftUI

struct AboutView: View {
    private static let repository = URL(string: "https://github.com/pyrlyn/ketch")

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }

    var body: some View {
        VStack(spacing: 12) {
            Image(nsImage: NSApplication.shared.applicationIconImage)
                .resizable().frame(width: 64, height: 64)
            Text("Ketch").font(.title.bold())
            Text("Version \(version)").foregroundStyle(.secondary)
            Text("Installs command-line tools and apps straight from GitHub releases.")
                .multilineTextAlignment(.center)
            VStack(alignment: .leading, spacing: 4) {
                Text("Licensed under any of, at your choice:").font(.headline)
                Text("• GNU GPL-3.0-or-later")
                Text("• the ketch Royalty-free License")
                Text("• a commercial licence")
            }
            if let repository = Self.repository {
                Link("github.com/pyrlyn/ketch", destination: repository)
            }
        }
        .padding(24)
        .frame(width: 360)
    }
}
