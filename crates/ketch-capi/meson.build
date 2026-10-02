# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# The Vala side of ketch-capi: tests/capi.vala, built against vapi/ketch.vapi
# and the library cargo built, and run as a Meson test. `just capi-test`.
project('ketch-capi', 'vala', 'c', meson_version: '>= 1.0')

capi_dir = get_option('capi_dir')
if capi_dir == ''
  error('pass -Dcapi_dir=<cargo target dir>/debug, where libketch_capi was built')
endif

ketch = declare_dependency(
  dependencies: [
    meson.get_compiler('vala').find_library('ketch', dirs: meson.current_source_dir() / 'vapi'),
    meson.get_compiler('c').find_library('ketch_capi', dirs: capi_dir),
  ],
  include_directories: include_directories('include'),
)

capi_test = executable(
  'capi-test',
  'tests/capi.vala',
  dependencies: [
    dependency('glib-2.0'),
    dependency('gobject-2.0'),
    dependency('json-glib-1.0'),
    ketch,
  ],
)

test('capi', capi_test, env: {'LD_LIBRARY_PATH': capi_dir, 'DYLD_LIBRARY_PATH': capi_dir}, timeout: 300)
