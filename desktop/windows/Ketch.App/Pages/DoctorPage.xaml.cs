// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Doctor: the checks `ketch doctor` runs, and when they last ran.

using System.ComponentModel;
using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp.Pages;

public sealed partial class DoctorPage : Page
{
    public DoctorPage()
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
        if (e.PropertyName == nameof(KetchStore.Findings))
        {
            Render();
        }
    }

    private void Render()
    {
        var store = App.Store;
        Findings.ItemsSource = store.Findings;
        Summary.Text = store.Findings.Count == 0
            ? "No checks have run yet"
            : store.ProblemCount == 0 ? "No problems found" : Fmt.Plural(store.ProblemCount, "problem") + " found";
    }

    private async void OnRun(object sender, RoutedEventArgs e) => await App.Store.RunDoctorAsync();
}
