// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// The shell: title bar, navigation, and the things that belong to no page,
// namely the busy and error bars, the running operation and the questions the
// core waits on (each a ContentDialog).

using System.ComponentModel;
using Ketch.AppCore;
using KetchApp.Pages;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace KetchApp;

public sealed partial class MainWindow : Window
{
    private static readonly Dictionary<string, Type> PageTypes = new()
    {
        ["installed"] = typeof(InstalledPage),
        ["discover"] = typeof(DiscoverPage),
        ["updates"] = typeof(UpdatesPage),
        ["activity"] = typeof(ActivityPage),
        ["doctor"] = typeof(DoctorPage),
        ["settings"] = typeof(SettingsPage),
    };

    private bool asking;

    public MainWindow()
    {
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        Title = "Ketch";

        App.Store.PropertyChanged += OnStoreChanged;
        Nav.SelectedItem = Nav.MenuItems[0];
        ContentFrame.Navigate(typeof(InstalledPage));
        ShowBars();
        ShowBadges();
        _ = App.Store.RefreshAsync();
        _ = App.Store.RunDoctorAsync();
    }

    /// <summary>The element dialogs and flyouts anchor to.</summary>
    public XamlRoot? DialogRoot => Root.XamlRoot;

    /// <summary>Applies the theme the user chose; the system setting (and so high contrast) follows when it is Default.</summary>
    public void SetTheme(ElementTheme theme) => Root.RequestedTheme = theme;

    /// <summary>Opens a package's page inside the navigation.</summary>
    public void OpenPackage(string name) => ContentFrame.Navigate(typeof(PackageDetailPage), name);

    // ItemInvoked, not SelectionChanged: choosing the page that is already
    // selected has to leave a package's page and return to the list.
    private void OnNavigate(NavigationView sender, NavigationViewItemInvokedEventArgs args)
    {
        var tag = args.IsSettingsInvoked ? "settings" : (args.InvokedItemContainer?.Tag as string);
        if (tag is not null && PageTypes.TryGetValue(tag, out var page))
        {
            ContentFrame.Navigate(page);
        }
    }

    private void OnPaneToggleRequested(TitleBar sender, object args) => Nav.IsPaneOpen = !Nav.IsPaneOpen;

    private void OnRetry(object sender, RoutedEventArgs e) => _ = App.Store.RetryBusyAsync();

    private void OnCancel(object sender, RoutedEventArgs e) => App.Store.Cancel();

    private void OnErrorClosed(InfoBar sender, InfoBarClosedEventArgs args) => App.Store.DismissError();

    private void OnStoreChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(KetchStore.Busy) or nameof(KetchStore.ErrorMessage) or nameof(KetchStore.Activity):
                ShowBars();
                break;
            case nameof(KetchStore.PendingUpgradeCount) or nameof(KetchStore.ProblemCount):
                ShowBadges();
                break;
            case nameof(KetchStore.PendingChoice) or nameof(KetchStore.PendingStop):
                _ = AskAsync();
                break;
        }
    }

    private void ShowBars()
    {
        var store = App.Store;
        BusyBar.IsOpen = store.Busy is not null;
        BusyBar.Message = store.Busy?.Message ?? "";

        var activity = store.Activity;
        ActivityBar.IsOpen = activity is not null;
        ActivityBar.Title = activity?.Title ?? "";
        ActivityProgress.Value = activity?.Fraction ?? 0;
        ActivityStatus.Text = activity?.Status ?? "";
        CancelButton.IsEnabled = activity is { IsCancelling: false };

        ErrorBar.IsOpen = store.ErrorMessage is not null;
        ErrorBar.Message = store.ErrorMessage ?? "";
    }

    private void ShowBadges()
    {
        var updates = App.Store.PendingUpgradeCount;
        UpdatesBadge.Value = updates;
        UpdatesBadge.Visibility = Fmt.Show(updates > 0);

        var problems = App.Store.ProblemCount;
        DoctorBadge.Value = problems;
        DoctorBadge.Visibility = Fmt.Show(problems > 0);
    }

    /// <summary>
    /// Shows the question the core is waiting on. One dialog at a time: the
    /// store holds the next question until this one is answered, so a second
    /// change while a dialog is open finds <c>asking</c> set and waits its turn.
    /// </summary>
    private async Task AskAsync()
    {
        if (asking)
        {
            return;
        }

        asking = true;
        try
        {
            while (App.Store.PendingChoice is { } || App.Store.PendingStop is { })
            {
                if (App.Store.PendingChoice is { } choice)
                {
                    var picked = await Dialogs.ChooseBinaryAsync(Root, choice);
                    App.Store.AnswerChoice(picked);
                }
                else if (App.Store.PendingStop is { } stop)
                {
                    App.Store.AnswerStop(await Dialogs.StopProcessesAsync(Root, stop));
                }
            }
        }
        finally
        {
            asking = false;
        }
    }
}
