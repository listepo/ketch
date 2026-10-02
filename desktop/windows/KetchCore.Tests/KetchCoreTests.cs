// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// ketch-ffi as the Windows app will call it: through the C# bindings
// uniffi-bindgen-cs generates, against a scratch root, never ~/.ketch.

using Ketch.Ffi;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace KetchCore.Tests;

[TestClass]
public sealed class KetchCoreTests
{
    /// A throwaway directory holding the root and a one-file local package.
    private sealed class Scratch : IDisposable
    {
        public string Dir { get; } =
            Path.Combine(Path.GetTempPath(), "ketch-cs-tests-" + Guid.NewGuid().ToString("N"));

        public string Root => Path.Combine(Dir, "root");

        public string Payload()
        {
            var file = Path.Combine(Dir, "payload", "hello");
            Directory.CreateDirectory(Path.GetDirectoryName(file)!);
            File.WriteAllText(file, "#!/bin/sh\necho hi\n");
            return "local:" + file;
        }

        public void Dispose()
        {
            if (Directory.Exists(Dir))
            {
                Directory.Delete(Dir, recursive: true);
            }
        }
    }

    [TestMethod]
    public void TheLibraryReportsItsVersion()
    {
        Assert.IsFalse(string.IsNullOrEmpty(KetchFfiMethods.KetchVersion()));
    }

    [TestMethod]
    public void AScratchRootHasNothingInstalled()
    {
        using var scratch = new Scratch();
        using var core = new Ketch.Ffi.KetchCore(scratch.Root);
        Assert.AreEqual(0, core.Installed().Length);
    }

    [TestMethod]
    public void DoctorReportsItsChecks()
    {
        using var scratch = new Scratch();
        using var core = new Ketch.Ffi.KetchCore(scratch.Root);
        var checks = core.Doctor(null);
        Assert.IsTrue(checks.Length > 0);
        Assert.IsTrue(checks.All(c => !string.IsNullOrEmpty(c.Name)));
    }

    [TestMethod]
    public void ACancelledInstallThrowsCancelledAndPlacesNothing()
    {
        using var scratch = new Scratch();
        using var core = new Ketch.Ffi.KetchCore(scratch.Root);
        using var cancel = new CancelToken();
        cancel.Cancel();
        Assert.ThrowsExactly<KetchException.Cancelled>(() =>
            core.Install([scratch.Payload()], new InstallOptions(), null, null, cancel));
        Assert.AreEqual(0, core.Installed().Length);
    }

    /// Collects every event a call reports, from whichever thread reports it.
    private sealed class Recorder : Reporter
    {
        private readonly List<Event> received = [];

        public Event[] Received
        {
            get { lock (received) { return [.. received]; } }
        }

        public void Event(Event @event)
        {
            lock (received) { received.Add(@event); }
        }
    }

    [TestMethod]
    public void AnInstallReportsItsStagesToAForeignReporter()
    {
        using var scratch = new Scratch();
        using var core = new Ketch.Ffi.KetchCore(scratch.Root);
        var recorder = new Recorder();
        var placed = core.Install([scratch.Payload()], new InstallOptions(Link: false), recorder, null, null);
        Assert.AreEqual(1, placed.Length);
        Assert.IsTrue(recorder.Received.OfType<Event.Step>().Any());
        Assert.AreEqual(placed[0].Package.Name, core.Installed().Single().Name);
    }
}
