/*
 * Copyright (c) 2026 Ivan Tugay
 * SPDX-License-Identifier: GPL-3.0-or-later
 * Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html
 *
 * ketch-capi for Vala: include/ketch.h, bound by hand.
 *
 * Every call answers with one JSON string, {"ok": <value>} or
 * {"error": {"kind": ..., "message": ...}}; parse it with json-glib.
 * schema/payloads.schema.json describes each value, and the comment on each
 * function in include/ketch.h names which one `ok` holds. The answer is
 * malloc'ed, so Vala's g_free is the right way to release it.
 *
 * Callbacks may run on any thread, and only until the call they were passed
 * to returns, so unowned delegates are all they need. The calls block: run
 * them off the main loop and hand events back with Idle.add.
 */
[CCode (cheader_filename = "ketch.h")]
namespace Ketch {
    /* One progress event, a JSON `Event`. */
    [CCode (cname = "KetchEventFn", has_target = true)]
    public delegate void EventFunc (string event);

    /* Which of `candidates` (a JSON array of strings) to link for `package`:
     * an index into it, or a negative number for no answer. */
    [CCode (cname = "KetchChooseFn", has_target = true)]
    public delegate int64 ChooseFunc (string package, string candidates);

    /* Whether to stop `holders` (a JSON array of `Holder`). */
    [CCode (cname = "KetchStopFn", has_target = true)]
    public delegate bool StopFunc (string holders);

    [CCode (cname = "ketch_version")]
    public string version ();

    /* A flag a call checks; trip it from any thread. */
    [Compact]
    [CCode (cname = "KetchCancel", free_function = "ketch_cancel_free")]
    public class Cancel {
        [CCode (cname = "ketch_cancel_new")]
        public Cancel ();
        [CCode (cname = "ketch_cancel_cancel")]
        public void cancel ();
        [CCode (cname = "ketch_cancel_is_cancelled")]
        public bool is_cancelled ();
    }

    /* A ketch root; `root` null means KETCH_ROOT, else ~/.ketch. Safe to
     * use from several threads at once. */
    [Compact]
    [CCode (cname = "KetchCore", free_function = "ketch_core_free")]
    public class Core {
        [CCode (cname = "ketch_core_new")]
        public Core (string? root);

        [CCode (cname = "ketch_root")]
        public string root ();
        [CCode (cname = "ketch_installed")]
        public string installed ();
        [CCode (cname = "ketch_search")]
        public string search (string query, uint32 limit, EventFunc? report);
        [CCode (cname = "ketch_outdated")]
        public string outdated (EventFunc? report);
        [CCode (cname = "ketch_install")]
        public string install ([CCode (array_length_type = "size_t")] string[] specs, string? options,
                               EventFunc? report, ChooseFunc? choose, StopFunc? stop, Cancel? cancel);
        [CCode (cname = "ketch_upgrade")]
        public string upgrade ([CCode (array_length_type = "size_t")] string[] names,
                               EventFunc? report, ChooseFunc? choose, StopFunc? stop, Cancel? cancel);
        [CCode (cname = "ketch_uninstall")]
        public string uninstall ([CCode (array_length_type = "size_t")] string[] names,
                                 EventFunc? report, ChooseFunc? choose, StopFunc? stop, Cancel? cancel);
        [CCode (cname = "ketch_changelog")]
        public string changelog (string package, string? version, EventFunc? report);
        [CCode (cname = "ketch_changelog_range")]
        public string changelog_range (string package, string? from, string? to, EventFunc? report);
        [CCode (cname = "ketch_doctor")]
        public string doctor (EventFunc? report);
        [CCode (cname = "ketch_doctor_fix")]
        public string doctor_fix (EventFunc? report);
        [CCode (cname = "ketch_history")]
        public string history (string? package, uint32 limit);
        [CCode (cname = "ketch_info")]
        public string info (string package, EventFunc? report);
        [CCode (cname = "ketch_pin")]
        public string pin ([CCode (array_length_type = "size_t")] string[] names);
        [CCode (cname = "ketch_unpin")]
        public string unpin ([CCode (array_length_type = "size_t")] string[] names);
        [CCode (cname = "ketch_rollback")]
        public string rollback (string package, string? to, EventFunc? report, ChooseFunc? choose, StopFunc? stop);
        /* A negative `keep` leaves the retention setting as it is. */
        [CCode (cname = "ketch_prune")]
        public string prune ([CCode (array_length_type = "size_t")] string[] names, int64 keep, EventFunc? report);
        [CCode (cname = "ketch_registry_refresh")]
        public string registry_refresh (EventFunc? report);
        [CCode (cname = "ketch_path_status")]
        public string path_status ();
        [CCode (cname = "ketch_path_install")]
        public string path_install (bool dry_run, EventFunc? report);
        [CCode (cname = "ketch_config")]
        public string config ();
    }
}
