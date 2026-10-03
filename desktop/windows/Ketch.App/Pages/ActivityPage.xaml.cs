// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Activity: the log of what ketch did, newest at the bottom.

using System.ComponentModel;
using Ketch.AppCore;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp.Pages;

public sealed partial class ActivityPage : Page
{
    public ActivityPage()
    {
        InitializeComponent();
        Loaded += (_, _) =>
        {
            App.Store.PropertyChanged += OnStoreChanged;
            Render();
        };
        Unloaded += (_, _) => App.Store.PropertyChanged -= OnStoreChanged;
    }

    private void OnStoreChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(KetchStore.Log))
        {
            Render();
        }
    }

    private void Render()
    {
        var log = App.Store.Log;
        Entries.ItemsSource = log;
        Entries.Visibility = Fmt.ShowWhenAny(log.Count);
        Empty.Visibility = Fmt.ShowWhenEmpty(log.Count);
        if (log.Count > 0)
        {
            Entries.ScrollIntoView(log[^1]);
        }
    }
}
