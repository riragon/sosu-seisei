fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Sosu-Seisei Settings",
        options,
        Box::new(|cc| Ok(Box::new(sosu_seisei_sieve::app::MyApp::new(cc)))),
    );
}
