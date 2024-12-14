// Copyright (c) 2024 riragon
//
// This software is released under the MIT License.
// See LICENSE file in the project root directory for more information.

fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Sosu-Seisei Settings",
        options,
        Box::new(|cc| Ok(Box::new(sosu_seisei_sieve::app::MyApp::new(cc)))),
    );
}
