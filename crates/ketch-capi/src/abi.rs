// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! The C ABI itself: the exported functions, the two handles and the callback
//! adapters, which is what `include/ketch.h` is generated from.
//!
//! The only module that may hold unsafe code, and it holds as little as the
//! boundary needs: reading the caller's strings, handing out `malloc`ed
//! answers, and carrying `user_data` across threads. Handles cross as `Box`
//! and `Option<&T>`, and callbacks as `Option<extern "C" fn>`, which the
//! compiler already knows are nullable pointers, so taking and freeing them is
//! safe Rust. The caller's side of the contract is the crate's `# Safety`.
//!
//! Each function's comment names what its envelope's `ok` holds, a type from
//! `schema/payloads.schema.json`.

// `#[no_mangle]` and the unsafe blocks below; nowhere else in the crate.
#![allow(unsafe_code)]
// A C call takes its callbacks flat, one function pointer and one `user_data`
// each, because that is how a Vala delegate with a target crosses.
#![allow(clippy::too_many_arguments)]

use crate::{invalid, respond};
use ketch_ffi::{CancelToken, Decider, Event, Holder, InstallOptions, KetchError, Reporter};
use serde::Serialize;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;
use std::sync::Arc;

/// A ketch root opened for calls.
pub struct KetchCore(Arc<ketch_ffi::KetchCore>);

/// A flag the caller keeps and the calls it is passed to check; trip it with
/// `ketch_cancel_cancel` from any thread.
pub struct KetchCancel(Arc<CancelToken>);

/// Receives each progress event of a call as JSON, an `Event`. NULL: no
/// reporting.
pub type KetchEventFn = Option<extern "C" fn(event: *const c_char, user_data: *mut c_void)>;

/// Picks which of several binaries sharing a package's name to link:
/// `candidates` is a JSON array of strings; answer an index into it, or a
/// negative number to leave it to the fixed rules. NULL: never asked.
pub type KetchChooseFn = Option<
    extern "C" fn(package: *const c_char, candidates: *const c_char, user_data: *mut c_void) -> i64,
>;

/// Whether to stop the processes that hold files about to be replaced:
/// `holders` is a JSON array of `Holder`. `false` leaves them running. NULL:
/// always `false`.
pub type KetchStopFn =
    Option<extern "C" fn(holders: *const c_char, user_data: *mut c_void) -> bool>;

/// The caller's `user_data`, carried to whichever thread reports.
struct UserData(*mut c_void);

// SAFETY: the pointer is never dereferenced in Rust, only handed back to the
// caller's own callbacks, and the crate's `# Safety` has the caller promise it
// is usable from any thread for the length of the call.
unsafe impl Send for UserData {}
// SAFETY: as for `Send`; nothing here reads through the pointer.
unsafe impl Sync for UserData {}

struct CReporter {
    call: KetchEventFn,
    data: UserData,
}

impl Reporter for CReporter {
    fn event(&self, event: Event) {
        if let (Some(call), Some(json)) = (self.call, c_json(&event)) {
            call(json.as_ptr(), self.data.0);
        }
    }
}

struct CDecider {
    choose: KetchChooseFn,
    choose_data: UserData,
    stop: KetchStopFn,
    stop_data: UserData,
}

impl Decider for CDecider {
    fn choose_binary(&self, package: String, candidates: Vec<String>) -> Option<u32> {
        let call = self.choose?;
        let package = CString::new(package).ok()?;
        let candidates = c_json(&candidates)?;
        u32::try_from(call(
            package.as_ptr(),
            candidates.as_ptr(),
            self.choose_data.0,
        ))
        .ok()
    }

    fn stop_processes(&self, holders: Vec<Holder>) -> bool {
        let Some(call) = self.stop else {
            return false;
        };
        c_json(&holders).is_some_and(|json| call(json.as_ptr(), self.stop_data.0))
    }
}

/// `value` as a C string for a callback. JSON escapes NUL, so this fails only
/// when serde_json does.
fn c_json<T: Serialize>(value: &T) -> Option<CString> {
    CString::new(serde_json::to_string(value).ok()?).ok()
}

fn reporter(call: KetchEventFn, data: *mut c_void) -> Option<Arc<dyn Reporter>> {
    call?;
    Some(Arc::new(CReporter {
        call,
        data: UserData(data),
    }))
}

fn decider(
    choose: KetchChooseFn,
    choose_data: *mut c_void,
    stop: KetchStopFn,
    stop_data: *mut c_void,
) -> Option<Arc<dyn Decider>> {
    if choose.is_none() && stop.is_none() {
        return None;
    }
    Some(Arc::new(CDecider {
        choose,
        choose_data: UserData(choose_data),
        stop,
        stop_data: UserData(stop_data),
    }))
}

fn cancel(token: Option<&KetchCancel>) -> Option<Arc<CancelToken>> {
    token.map(|t| Arc::clone(&t.0))
}

fn this(core: Option<&KetchCore>) -> Result<&ketch_ffi::KetchCore, KetchError> {
    core.map(|c| &*c.0).ok_or_else(|| invalid("core is NULL"))
}

/// A string argument that may be NULL.
///
/// # Safety
///
/// `p` is NULL or a NUL-terminated string valid for the call.
unsafe fn optional_text(p: *const c_char, what: &str) -> Result<Option<String>, KetchError> {
    if p.is_null() {
        return Ok(None);
    }
    // SAFETY: not NULL, and this function's contract makes it a NUL-terminated
    // string that outlives the borrow.
    let text = unsafe { CStr::from_ptr(p) };
    text.to_str()
        .map(|t| Some(t.to_owned()))
        .map_err(|_| invalid(&format!("{what} is not UTF-8")))
}

/// A string argument that must be there.
///
/// # Safety
///
/// As [`optional_text`].
unsafe fn text(p: *const c_char, what: &str) -> Result<String, KetchError> {
    // SAFETY: this function's contract is `optional_text`'s.
    unsafe { optional_text(p, what) }?.ok_or_else(|| invalid(&format!("{what} is NULL")))
}

/// A string array argument.
///
/// # Safety
///
/// `p` is NULL with `len` 0, or points at `len` strings each as [`text`]
/// wants.
unsafe fn texts(
    p: *const *const c_char,
    len: usize,
    what: &str,
) -> Result<Vec<String>, KetchError> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if p.is_null() {
        return Err(invalid(&format!("{what} is NULL")));
    }
    // SAFETY: not NULL, and this function's contract puts `len` initialised
    // pointers there for the length of the call.
    let items = unsafe { std::slice::from_raw_parts(p, len) };
    items
        .iter()
        // SAFETY: each element is a string as `text` wants, by the same contract.
        .map(|&item| unsafe { text(item, what) })
        .collect()
}

/// `json` copied into `malloc`ed memory, so either `ketch_string_free` or
/// GLib's `g_free` can free it. NULL only when memory ran out.
fn answer(json: String) -> *mut c_char {
    let bytes = json.as_bytes();
    // SAFETY: `malloc` has no preconditions; its NULL is checked below.
    let out = unsafe { libc::malloc(bytes.len() + 1) }.cast::<u8>();
    if out.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `out` holds `len + 1` writable bytes and is fresh memory, so it
    // cannot overlap `bytes`.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        out.add(bytes.len()).write(0);
    }
    out.cast()
}

/// The version of ketch this library was built from. `ok`: a string.
#[no_mangle]
pub extern "C" fn ketch_version() -> *mut c_char {
    answer(respond(|| Ok(ketch_ffi::ketch_version())))
}

/// Free an answer from any function in this library. NULL is ignored.
///
/// # Safety
///
/// `answer` is NULL or a string this library returned, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn ketch_string_free(answer: *mut c_char) {
    // SAFETY: by this function's contract the string came from `answer`, so
    // from `malloc`, or is NULL, which `free` ignores.
    unsafe { libc::free(answer.cast()) }
}

/// Open the ketch root `root`, or the default root (`KETCH_ROOT`, else
/// `~/.ketch`) when NULL. Nothing is read until the first call. NULL when
/// `root` is not UTF-8.
///
/// # Safety
///
/// `root` is NULL or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn ketch_core_new(root: *const c_char) -> Option<Box<KetchCore>> {
    // SAFETY: this function's contract is `optional_text`'s.
    let root = unsafe { optional_text(root, "root") }.ok()?;
    Some(Box::new(KetchCore(ketch_ffi::KetchCore::new(root))))
}

/// Close a root opened with `ketch_core_new`. NULL is ignored.
///
/// # Safety
///
/// `core` came from `ketch_core_new`, is not freed yet, and no call is using it.
#[no_mangle]
pub unsafe extern "C" fn ketch_core_free(core: Option<Box<KetchCore>>) {
    drop(core);
}

/// A new cancellation flag, not tripped.
#[no_mangle]
pub extern "C" fn ketch_cancel_new() -> Box<KetchCancel> {
    Box::new(KetchCancel(CancelToken::new()))
}

/// Trip `cancel`: every call holding it stops at its next check and answers
/// `cancelled`. Safe from any thread.
///
/// # Safety
///
/// `cancel` came from `ketch_cancel_new` and is not freed yet.
#[no_mangle]
pub unsafe extern "C" fn ketch_cancel_cancel(cancel: Option<&KetchCancel>) {
    if let Some(cancel) = cancel {
        cancel.0.cancel();
    }
}

/// Whether `cancel` has been tripped. NULL is never tripped.
///
/// # Safety
///
/// As `ketch_cancel_cancel`.
#[no_mangle]
pub unsafe extern "C" fn ketch_cancel_is_cancelled(cancel: Option<&KetchCancel>) -> bool {
    cancel.is_some_and(|c| c.0.is_cancelled())
}

/// Free a flag from `ketch_cancel_new`. NULL is ignored.
///
/// # Safety
///
/// `cancel` came from `ketch_cancel_new`, is not freed yet, and no call is
/// using it.
#[no_mangle]
pub unsafe extern "C" fn ketch_cancel_free(cancel: Option<Box<KetchCancel>>) {
    drop(cancel);
}

/// The root this core works in. `ok`: a string.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_root(core: Option<&KetchCore>) -> *mut c_char {
    answer(respond(|| this(core)?.root()))
}

/// Every installed package, by name. `ok`: an array of `Package`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_installed(core: Option<&KetchCore>) -> *mut c_char {
    answer(respond(|| this(core)?.installed()))
}

/// Packages and repositories matching `query`, at most `limit` repositories.
/// `ok`: a `SearchResults`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_search(
    core: Option<&KetchCore>,
    query: *const c_char,
    limit: u32,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `query` a string.
        let query = unsafe { text(query, "query") }?;
        this(core)?.search(query, limit, reporter(event, event_data))
    }))
}

/// Installed packages with a newer release, pinned ones marked.
/// `ok`: an array of `Upgrade`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_outdated(
    core: Option<&KetchCore>,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        this(core)?.outdated(reporter(event, event_data))
    }))
}

/// Install `specs`. `options` is a JSON `InstallOptions`, any field left out
/// at its default, or NULL for all defaults. `ok`: an array of `Installed`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_install(
    core: Option<&KetchCore>,
    specs: *const *const c_char,
    specs_len: usize,
    options: *const c_char,
    event: KetchEventFn,
    event_data: *mut c_void,
    choose: KetchChooseFn,
    choose_data: *mut c_void,
    stop: KetchStopFn,
    stop_data: *mut c_void,
    cancel_token: Option<&KetchCancel>,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `specs` `specs_len` strings.
        let specs = unsafe { texts(specs, specs_len, "specs") }?;
        // SAFETY: the crate's rules make `options` NULL or a string.
        let options = match unsafe { optional_text(options, "options") }? {
            Some(json) => serde_json::from_str::<InstallOptions>(&json)
                .map_err(|e| invalid(&format!("options: {e}")))?,
            None => InstallOptions::default(),
        };
        this(core)?.install(
            specs,
            options,
            reporter(event, event_data),
            decider(choose, choose_data, stop, stop_data),
            cancel(cancel_token),
        )
    }))
}

/// Upgrade `names`, or every package with a newer release when empty; pinned
/// packages are skipped. `ok`: an array of `Installed`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_upgrade(
    core: Option<&KetchCore>,
    names: *const *const c_char,
    names_len: usize,
    event: KetchEventFn,
    event_data: *mut c_void,
    choose: KetchChooseFn,
    choose_data: *mut c_void,
    stop: KetchStopFn,
    stop_data: *mut c_void,
    cancel_token: Option<&KetchCancel>,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `names` `names_len` strings.
        let names = unsafe { texts(names, names_len, "names") }?;
        this(core)?.upgrade(
            names,
            reporter(event, event_data),
            decider(choose, choose_data, stop, stop_data),
            cancel(cancel_token),
        )
    }))
}

/// Uninstall `names`. `ok`: an array of the `Package`s removed.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_uninstall(
    core: Option<&KetchCore>,
    names: *const *const c_char,
    names_len: usize,
    event: KetchEventFn,
    event_data: *mut c_void,
    choose: KetchChooseFn,
    choose_data: *mut c_void,
    stop: KetchStopFn,
    stop_data: *mut c_void,
    cancel_token: Option<&KetchCancel>,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `names` `names_len` strings.
        let names = unsafe { texts(names, names_len, "names") }?;
        this(core)?.uninstall(
            names,
            reporter(event, event_data),
            decider(choose, choose_data, stop, stop_data),
            cancel(cancel_token),
        )
    }))
}

/// What changed in `package` at `version`, or at its newest release when
/// NULL. `ok`: a `Changelog`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_changelog(
    core: Option<&KetchCore>,
    package: *const c_char,
    version: *const c_char,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `package` a string.
        let package = unsafe { text(package, "package") }?;
        // SAFETY: the crate's rules make `version` NULL or a string.
        let version = unsafe { optional_text(version, "version") }?;
        this(core)?.changelog(package, version, reporter(event, event_data))
    }))
}

/// The release notes after `from` (NULL: the installed version) up to and
/// including `to` (NULL: the newest), newest first. `ok`: an array of
/// `Changelog`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_changelog_range(
    core: Option<&KetchCore>,
    package: *const c_char,
    from: *const c_char,
    to: *const c_char,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `package` a string.
        let package = unsafe { text(package, "package") }?;
        // SAFETY: the crate's rules make `from` NULL or a string.
        let from = unsafe { optional_text(from, "from") }?;
        // SAFETY: the crate's rules make `to` NULL or a string.
        let to = unsafe { optional_text(to, "to") }?;
        this(core)?.changelog_range(package, from, to, reporter(event, event_data))
    }))
}

/// `ketch doctor`'s checks. `ok`: an array of `Check`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_doctor(
    core: Option<&KetchCore>,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| this(core)?.doctor(reporter(event, event_data))))
}

/// `ketch doctor --fix`: put the bin dir on `PATH` for the shells in use.
/// `ok`: an array of `PathChange`, empty when nothing needed fixing.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_doctor_fix(
    core: Option<&KetchCore>,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        this(core)?.doctor_fix(reporter(event, event_data))
    }))
}

/// The newest `limit` events from `stats.db`, for `package` or for all when
/// NULL. `ok`: an array of `HistoryEvent`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_history(
    core: Option<&KetchCore>,
    package: *const c_char,
    limit: u32,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `package` NULL or a string.
        let package = unsafe { optional_text(package, "package") }?;
        this(core)?.history(package, limit)
    }))
}

/// `ketch info`. `ok`: a `PackageInfo`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_info(
    core: Option<&KetchCore>,
    package: *const c_char,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `package` a string.
        let package = unsafe { text(package, "package") }?;
        this(core)?.info(package, reporter(event, event_data))
    }))
}

/// Pin `names`, or every installed package when empty. `ok`: an array of
/// `Package`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_pin(
    core: Option<&KetchCore>,
    names: *const *const c_char,
    names_len: usize,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `names` `names_len` strings.
        let names = unsafe { texts(names, names_len, "names") }?;
        this(core)?.pin(names)
    }))
}

/// Unpin `names`, or every installed package when empty. `ok`: an array of
/// `Package`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_unpin(
    core: Option<&KetchCore>,
    names: *const *const c_char,
    names_len: usize,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `names` `names_len` strings.
        let names = unsafe { texts(names, names_len, "names") }?;
        this(core)?.unpin(names)
    }))
}

/// Go back to the retained version `to`, or the newest retained one when
/// NULL. `ok`: an `Installed`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_rollback(
    core: Option<&KetchCore>,
    package: *const c_char,
    to: *const c_char,
    event: KetchEventFn,
    event_data: *mut c_void,
    choose: KetchChooseFn,
    choose_data: *mut c_void,
    stop: KetchStopFn,
    stop_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `package` a string.
        let package = unsafe { text(package, "package") }?;
        // SAFETY: the crate's rules make `to` NULL or a string.
        let to = unsafe { optional_text(to, "to") }?;
        this(core)?.rollback(
            package,
            to,
            reporter(event, event_data),
            decider(choose, choose_data, stop, stop_data),
        )
    }))
}

/// Remove retained versions of `names` (every package when empty) beyond
/// `keep`, which also becomes the retention setting; a negative `keep` uses
/// the setting as it is. `ok`: an array of `Pruned`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_prune(
    core: Option<&KetchCore>,
    names: *const *const c_char,
    names_len: usize,
    keep: i64,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        // SAFETY: the crate's rules make `names` `names_len` strings.
        let names = unsafe { texts(names, names_len, "names") }?;
        let keep = match keep {
            k if k < 0 => None,
            k => Some(u32::try_from(k).map_err(|_| invalid("keep is too large"))?),
        };
        this(core)?.prune(names, keep, reporter(event, event_data))
    }))
}

/// Fetch the package registry again. `ok`: the number of packages in it.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_registry_refresh(
    core: Option<&KetchCore>,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        this(core)?.registry_refresh(reporter(event, event_data))
    }))
}

/// Whether the bin dir is on `PATH`, and each shell's setup. `ok`: a
/// `PathStatus`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_path_status(core: Option<&KetchCore>) -> *mut c_char {
    answer(respond(|| this(core)?.path_status()))
}

/// `ketch path install`; with `dry_run`, only what it would change.
/// `ok`: an array of `PathChange`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_path_install(
    core: Option<&KetchCore>,
    dry_run: bool,
    event: KetchEventFn,
    event_data: *mut c_void,
) -> *mut c_char {
    answer(respond(|| {
        this(core)?.path_install(dry_run, reporter(event, event_data))
    }))
}

/// The settings in effect for this root. `ok`: a `Settings`.
///
/// # Safety
///
/// The calling rules at the top of ketch-capi's `src/lib.rs`.
#[no_mangle]
pub unsafe extern "C" fn ketch_config(core: Option<&KetchCore>) -> *mut c_char {
    answer(respond(|| this(core)?.config()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use std::sync::Mutex;

    /// Parse an answer and free it, as a caller must.
    fn take(answer: *mut c_char) -> Value {
        assert!(!answer.is_null());
        // SAFETY: `answer` is a fresh, NUL-terminated answer from this library.
        let json = unsafe { CStr::from_ptr(answer) }
            .to_str()
            .unwrap()
            .to_owned();
        // SAFETY: `answer` came from this library and is freed once, here.
        unsafe { ketch_string_free(answer) };
        serde_json::from_str(&json).unwrap()
    }

    fn scratch() -> (tempfile::TempDir, Box<KetchCore>) {
        let dir = tempfile::tempdir().unwrap();
        let root = CString::new(dir.path().join("root").display().to_string()).unwrap();
        // SAFETY: `root` is a NUL-terminated string that outlives the call.
        let core = unsafe { ketch_core_new(root.as_ptr()) }.unwrap();
        (dir, core)
    }

    /// A one-file `local:` package under `dir`, as a C string.
    fn payload(dir: &std::path::Path) -> CString {
        let file = dir.join("payload").join("hello");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "#!/bin/sh\necho hi\n").unwrap();
        CString::new(format!("local:{}", file.display())).unwrap()
    }

    extern "C" fn record(event: *const c_char, user_data: *mut c_void) {
        // SAFETY: the tests pass a `Mutex<Vec<Value>>` that outlives the call.
        let seen = unsafe { &*user_data.cast::<Mutex<Vec<Value>>>() };
        // SAFETY: ketch-capi passes a NUL-terminated string valid for this call.
        let event = unsafe { CStr::from_ptr(event) }.to_str().unwrap();
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(event).unwrap());
    }

    /// Install `spec` with only a reporter, as the Vala test does.
    fn install(
        core: &KetchCore,
        spec: &CString,
        seen: &Mutex<Vec<Value>>,
        cancel: Option<&KetchCancel>,
    ) -> Value {
        let specs = [spec.as_ptr()];
        // SAFETY: one live string in `specs`; `seen` outlives the call.
        take(unsafe {
            ketch_install(
                Some(core),
                specs.as_ptr(),
                specs.len(),
                ptr::null(),
                Some(record),
                ptr::from_ref(seen).cast_mut().cast(),
                None,
                ptr::null_mut(),
                None,
                ptr::null_mut(),
                cancel,
            )
        })
    }

    #[test]
    fn a_scratch_root_has_nothing_installed_and_a_doctor_report() {
        let (_dir, core) = scratch();
        // SAFETY: `core` is live.
        let installed = take(unsafe { ketch_installed(Some(&core)) });
        assert_eq!(installed, json!({"ok": []}));
        // SAFETY: `core` is live and no callback is passed.
        let doctor = take(unsafe { ketch_doctor(Some(&core), None, ptr::null_mut()) });
        assert!(!doctor["ok"].as_array().unwrap().is_empty(), "{doctor}");
        // SAFETY: `core` came from `ketch_core_new` and nothing uses it now.
        unsafe { ketch_core_free(Some(core)) };
    }

    #[test]
    fn an_install_reports_to_the_callback_and_its_record_comes_back() {
        let (dir, core) = scratch();
        let seen = Mutex::new(Vec::new());
        let placed = install(&core, &payload(dir.path()), &seen, None);
        let name = placed["ok"][0]["package"]["name"]
            .as_str()
            .unwrap()
            .to_owned();
        let seen = seen.into_inner().unwrap();
        assert!(seen.iter().any(|e| e["type"] == "step"), "{seen:?}");

        // SAFETY: `core` is live.
        let installed = take(unsafe { ketch_installed(Some(&core)) });
        assert_eq!(installed["ok"][0]["name"], Value::from(name));
    }

    #[test]
    fn a_cancelled_install_answers_cancelled_and_places_nothing() {
        let (dir, core) = scratch();
        let cancel = ketch_cancel_new();
        // SAFETY: `cancel` is live.
        unsafe { ketch_cancel_cancel(Some(&cancel)) };
        // SAFETY: as above.
        assert!(unsafe { ketch_cancel_is_cancelled(Some(&cancel)) });
        let seen = Mutex::new(Vec::new());
        let answer = install(&core, &payload(dir.path()), &seen, Some(&cancel));
        assert_eq!(answer["error"]["type"], "cancelled", "{answer}");
        // SAFETY: `core` is live.
        let installed = take(unsafe { ketch_installed(Some(&core)) });
        assert_eq!(installed, json!({"ok": []}));
        // SAFETY: `cancel` came from `ketch_cancel_new` and nothing uses it now.
        unsafe { ketch_cancel_free(Some(cancel)) };
    }

    #[test]
    fn a_null_core_or_name_is_an_error_not_a_crash() {
        // SAFETY: NULL is what is being tested, and each function takes it.
        let answer = take(unsafe { ketch_installed(None) });
        assert_eq!(answer["error"]["message"], "ketch-capi: core is NULL");
        let (_dir, core) = scratch();
        // SAFETY: as above, for `package`.
        let answer = take(unsafe { ketch_info(Some(&core), ptr::null(), None, ptr::null_mut()) });
        assert_eq!(answer["error"]["message"], "ketch-capi: package is NULL");
        // SAFETY: NULL with a zero length is the documented empty array.
        let answer = take(unsafe { ketch_pin(Some(&core), ptr::null(), 0) });
        assert_eq!(answer, json!({"ok": []}));
    }

    #[test]
    fn bytes_that_are_not_utf8_are_an_error() {
        let (_dir, core) = scratch();
        let bad = CString::new(vec![0xff, 0xfe]).unwrap();
        // SAFETY: `bad` is NUL-terminated and outlives the call.
        let answer = take(unsafe { ketch_info(Some(&core), bad.as_ptr(), None, ptr::null_mut()) });
        assert_eq!(
            answer["error"]["message"],
            "ketch-capi: package is not UTF-8"
        );
        // SAFETY: as above.
        assert!(unsafe { ketch_core_new(bad.as_ptr()) }.is_none());
    }

    #[test]
    fn install_options_are_json_and_a_malformed_one_is_refused() {
        let (dir, core) = scratch();
        let spec = payload(dir.path());
        let specs = [spec.as_ptr()];
        let call = |options: &CString| {
            // SAFETY: every pointer is a live string or NULL for the call.
            take(unsafe {
                ketch_install(
                    Some(&core),
                    specs.as_ptr(),
                    1,
                    options.as_ptr(),
                    None,
                    ptr::null_mut(),
                    None,
                    ptr::null_mut(),
                    None,
                    ptr::null_mut(),
                    None,
                )
            })
        };
        let answer = call(&CString::new("{\"link\": tru").unwrap());
        assert!(
            answer["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("ketch-capi: options:"),
            "{answer}"
        );
        let answer = call(&CString::new("{\"link\": false}").unwrap());
        assert!(answer["ok"].is_array(), "{answer}");
    }

    extern "C" fn second(
        _package: *const c_char,
        candidates: *const c_char,
        _: *mut c_void,
    ) -> i64 {
        // SAFETY: ketch-capi passes a NUL-terminated string valid for this call.
        let candidates = unsafe { CStr::from_ptr(candidates) }.to_str().unwrap();
        assert_eq!(candidates, r#"["a","b"]"#);
        1
    }

    extern "C" fn none(_: *const c_char, _: *const c_char, _: *mut c_void) -> i64 {
        -1
    }

    extern "C" fn agree(holders: *const c_char, _: *mut c_void) -> bool {
        // SAFETY: ketch-capi passes a NUL-terminated string valid for this call.
        let holders = unsafe { CStr::from_ptr(holders) }.to_str().unwrap();
        holders == r#"[{"pid":7,"path":"/bin/x"}]"#
    }

    #[test]
    fn the_decider_callbacks_get_json_and_a_negative_pick_is_no_answer() {
        let both = decider(Some(second), ptr::null_mut(), Some(agree), ptr::null_mut()).unwrap();
        assert_eq!(
            both.choose_binary("p".into(), vec!["a".into(), "b".into()]),
            Some(1)
        );
        let holder = Holder {
            pid: 7,
            path: "/bin/x".into(),
        };
        assert!(both.stop_processes(vec![holder.clone()]));

        let declining = decider(Some(none), ptr::null_mut(), None, ptr::null_mut()).unwrap();
        assert_eq!(declining.choose_binary("p".into(), vec!["a".into()]), None);
        assert!(!declining.stop_processes(vec![holder]));
        assert!(decider(None, ptr::null_mut(), None, ptr::null_mut()).is_none());
    }

    #[test]
    fn the_settings_name_the_root_it_was_opened_with() {
        let (dir, core) = scratch();
        // SAFETY: `core` is live.
        let settings = take(unsafe { ketch_config(Some(&core)) });
        assert_eq!(
            settings["ok"]["root"],
            Value::from(dir.path().join("root").display().to_string())
        );
        // SAFETY: `core` is live.
        let root = take(unsafe { ketch_root(Some(&core)) });
        assert_eq!(root["ok"], settings["ok"]["root"]);
        assert!(take(ketch_version())["ok"].is_string());
    }

    /// `value` against the schema's `$defs/<def>`, or `[<def>]` for an array.
    fn assert_matches(value: &Value, def: &str) {
        let mut schema: Value = serde_json::from_str(&crate::tests::payload_schema()).unwrap();
        let target = match def.strip_prefix('[').and_then(|d| d.strip_suffix(']')) {
            Some(item) => json!({"type": "array", "items": {"$ref": format!("#/$defs/{item}")}}),
            None => json!({"$ref": format!("#/$defs/{def}")}),
        };
        let defs = schema["$defs"].take();
        let validator =
            jsonschema::validator_for(&json!({"$defs": defs, "allOf": [target]})).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{def}: {errors:?} in {value}");
    }

    #[test]
    fn the_answers_match_the_payload_schema() {
        let (dir, core) = scratch();
        let seen = Mutex::new(Vec::new());
        let placed = install(&core, &payload(dir.path()), &seen, None);
        assert_matches(&placed["ok"], "[Installed]");
        for event in seen.into_inner().unwrap() {
            assert_matches(&event, "Event");
        }
        // SAFETY: `core` is live and no callback is passed.
        assert_matches(
            &take(unsafe { ketch_installed(Some(&core)) })["ok"],
            "[Package]",
        );
        // SAFETY: as above.
        let doctor = take(unsafe { ketch_doctor(Some(&core), None, ptr::null_mut()) });
        assert_matches(&doctor["ok"], "[Check]");
        // SAFETY: as above.
        assert_matches(
            &take(unsafe { ketch_config(Some(&core)) })["ok"],
            "Settings",
        );
        // SAFETY: as above.
        assert_matches(
            &take(unsafe { ketch_path_status(Some(&core)) })["ok"],
            "PathStatus",
        );
        // SAFETY: as above.
        let history = take(unsafe { ketch_history(Some(&core), ptr::null(), 10) });
        assert_matches(&history["ok"], "[HistoryEvent]");
        // SAFETY: NULL is the error being matched.
        assert_matches(
            &take(unsafe { ketch_installed(None) })["error"],
            "ErrorBody",
        );
    }
}
