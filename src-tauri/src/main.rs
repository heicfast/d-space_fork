// SPDX-License-Identifier: GPL-3.0-or-later
// DiskWizard — Windows data recovery wizard and safe disk cleanup.
// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    diskwizard_lib::run()
}
