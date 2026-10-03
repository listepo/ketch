// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Application start: builds the store on the fake core and opens the window.

using Ketch.AppCore;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace KetchApp;

public partial class App : Application
{
    private Window? window;

    public App()
    {
        InitializeComponent();
    }

    /// <summary>The fake core behind <see cref="Store"/>, for Settings to script.</summary>
    public static FakeKetchCore Core { get; } = FakeCoreSetup.Create();

    /// <summary>What every page binds to; events reach it through the UI thread's queue.</summary>
    public static KetchStore Store { get; private set; } = null!;

    public static MainWindow? Main { get; private set; }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var queue = DispatcherQueue.GetForCurrentThread();
        Store = new KetchStore(Core, action => queue.TryEnqueue(() => action()));
        Main = new MainWindow();
        window = Main;
        window.Activate();
    }
}
