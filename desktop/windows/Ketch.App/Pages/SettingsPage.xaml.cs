// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Settings: the theme, where ketch lives, and the fake core's scenario picker.
// Nothing is saved yet: unpackaged apps have no ApplicationData settings, and
// the real settings arrive with the live core (D12).

using Ketch.AppCore;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp.Pages;

public sealed partial class SettingsPage : Page
{
    private const string Simulated = "Simulated pipeline";

    private readonly IReadOnlyList<ContractScenario> scenarios =
        [.. FakeCoreSetup.Scenarios().Where(FakeCoreSetup.IsMutation)];

    private bool loading = true;

    public SettingsPage()
    {
        InitializeComponent();
        Root.Text = App.Store.Root;
        Theme.SelectedIndex = App.Main?.Content is FrameworkElement { RequestedTheme: var current }
            ? current switch { ElementTheme.Light => 1, ElementTheme.Dark => 2, _ => 0 }
            : 0;

        Scenario.Items.Add(Simulated);
        foreach (var scenario in scenarios)
        {
            Scenario.Items.Add(scenario.Name);
        }

        Scenario.SelectedIndex = 0;
        loading = false;
    }

    private void OnThemeChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!loading && Theme.SelectedItem is ComboBoxItem { Tag: string tag } && Enum.TryParse<ElementTheme>(tag, out var theme))
        {
            App.Main?.SetTheme(theme);
        }
    }

    private void OnScenarioChanged(object sender, SelectionChangedEventArgs e)
    {
        if (loading)
        {
            return;
        }

        App.Core.ClearScripted();
        if (Scenario.SelectedIndex > 0)
        {
            var scenario = scenarios[Scenario.SelectedIndex - 1];
            App.Core.Script(scenario);
            ScenarioText.Text = scenario.Description;
        }
        else
        {
            ScenarioText.Text = "";
        }
    }
}
