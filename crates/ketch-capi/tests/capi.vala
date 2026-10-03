/*
 * Copyright (c) 2026 Ivan Tugay
 * SPDX-License-Identifier: GPL-3.0-or-later
 * Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html
 *
 * ketch-capi as the Vala app will call it: through vapi/ketch.vapi, with
 * json-glib reading the answers, against a scratch root. Run by Meson
 * (`just capi-test`, and the ketch-capi job on Linux CI).
 */

/* The parsed envelope, or a failed test. */
Json.Object envelope (string answer) {
    var parser = new Json.Parser ();
    try {
        parser.load_from_data (answer);
    } catch (Error e) {
        error ("not JSON (%s): %s", e.message, answer);
    }
    return parser.get_root ().get_object ();
}

/* What `ok` holds, or a failed test when the call answered an error. */
Json.Node ok (string answer) {
    var reply = envelope (answer);
    if (!reply.has_member ("ok")) {
        error ("expected ok: %s", answer);
    }
    return reply.get_member ("ok");
}

void expect (bool claim, string what) {
    if (!claim) {
        error ("failed: %s", what);
    }
}

int main () {
    string dir;
    try {
        dir = DirUtils.make_tmp ("ketch-capi-XXXXXX");
    } catch (Error e) {
        error ("no scratch dir: %s", e.message);
    }
    var core = new Ketch.Core (Path.build_filename (dir, "root"));
    expect (ok (core.installed ()).get_array ().get_length () == 0, "a scratch root has nothing installed");
    expect (ok (core.doctor (null)).get_array ().get_length () > 0, "doctor reports its checks");

    var payload = Path.build_filename (dir, "hello");
    try {
        FileUtils.set_contents (payload, "#!/bin/sh\necho hi\n");
    } catch (Error e) {
        error ("no payload: %s", e.message);
    }
    string[] specs = { "local:" + payload };

    var cancel = new Ketch.Cancel ();
    cancel.cancel ();
    expect (cancel.is_cancelled (), "a tripped flag says so");
    var refused = envelope (core.install (specs, null, null, null, null, cancel));
    expect (refused.has_member ("error"), "a cancelled install is an error");
    expect (refused.get_object_member ("error").get_string_member ("type") == "cancelled",
            "the error is cancelled");
    expect (ok (core.installed ()).get_array ().get_length () == 0, "a cancelled install places nothing");

    // The same install, not cancelled, with a closure for a reporter: the
    // delegate's target is how the C user_data crosses.
    int steps = 0;
    var placed = ok (core.install (specs, "{\"link\": false}", (event) => {
        if (envelope_event_type (event) == "step") {
            steps++;
        }
    }, null, null, null)).get_array ();
    expect (placed.get_length () == 1, "the install placed one package");
    expect (steps > 0, "the reporter saw its steps");
    var name = placed.get_object_element (0).get_object_member ("package").get_string_member ("name");
    var installed = ok (core.installed ()).get_array ();
    expect (installed.get_object_element (0).get_string_member ("name") == name,
            "installed lists what install placed");

    print ("capi: installed, doctor, cancelled install and a reported install all answered\n");
    return 0;
}

/* An event's `type`, from the JSON a reporter receives. */
string envelope_event_type (string event) {
    var parser = new Json.Parser ();
    try {
        parser.load_from_data (event);
    } catch (Error e) {
        error ("event is not JSON (%s): %s", e.message, event);
    }
    return parser.get_root ().get_object ().get_string_member ("type");
}
