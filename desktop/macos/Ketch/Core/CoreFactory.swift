// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The one place that decides which core the app runs on. Until R9's
// `ketch-ffi` exists it is the fake; wiring the real core means adding a
// `LiveKetchCore` adapter over the generated `KetchCore` object and returning
// it here.

import Foundation

enum CoreFactory {
    static func make(root: URL = KetchRoot.resolve()) -> any KetchCoreProtocol {
        FakeKetchCore(root: root, stepDelay: .milliseconds(250))
    }
}
