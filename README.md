<h2>概要</h2>
Sosu-Seisei は、大規模な素数生成を支援するツールです。<br>
本ツールは、GUI を通じて 2 種類の手法による素数列挙を行えます。<br><br>

<h2>旧方式 (Sieve 法)</h2>
64ビット整数範囲内で動作する、従来のセグメント化エラトステネスの篩による素数生成手法。<br>
数値範囲を指定して、処理速度の速い篩法で素数を生成します。<br><br>

<h2>新方式 (Miller-Rabin 法)</h2>
任意精度整数 (num-bigint) に対応し、Miller-Rabin 素数判定法を用いて大きな素数を抽出します。<br>
非常に大きな値に対しても動作可能ですが、計算コストは範囲に応じて増大します。<br><br>
GUI を通して、prime_min と prime_max を指定し、範囲内の素数を primes.txt に出力します。<br><br>

<h2>使用クレート</h2>
本プロジェクトでは、以下の主なクレートを使用しています：<br><br>
rayon: 並列処理を容易にします（旧方式の篩計算で高速化に貢献）。<br>
bitvec: 素数計算用のビット操作を効率的に行うために利用。<br>
serde + toml: 設定ファイル(settings.txt)の読み書きに使用。<br>
egui + eframe: GUI 構築用フレームワーク。素数範囲や手法の選択を行うための簡易UIを提供。<br>
num-bigint, num-traits: 大きな整数を扱うためのクレート。Miller-Rabin 法で任意精度の数値範囲を扱うために利用。<br>
miller_rabin: Miller-Rabin 法による素数判定クレート。<br><br>

<h2>インストールと実行</h2>
リポジトリをクローンします:<br>
git clone https://github.com/riragon/sosu-seisei.git<br>
cd sosu-seisei<br><br>

2. リリースビルドで実行します:<br><br>
cargo run --release<br><br>

実行すると GUI ウィンドウが立ち上がります。<br>
もし `settings.txt` が存在しない場合は、自動的に作成されます。<br><br>

<h2>設定ファイル (settings.txt)</h2>
settings.txt は TOML 形式で、プログラムの動作を制御するためのパラメータを保持します。<br>
以下は設定可能なオプションの例です（デフォルト値は下記）：<br><br>

prime_cache_size (default: 100,000): 小さな素数を前処理する際のキャッシュサイズ（旧方式用）。<br>
segment_size (default: 10,000,000): セグメント範囲サイズ（旧方式用）。<br>
chunk_size (default: 16,384): 並列処理用チャンクサイズ（旧方式用）。<br>
writer_buffer_size (default: 8MB): ファイル書き出し時のバッファサイズ。<br>
prime_min (default: "1"): 素数探索の下限値（文字列形式で、任意精度整数可）。<br>
prime_max (default: "1000000"): 素数探索の上限値（文字列形式で、任意精度整数可）。<br>
miller_rabin_rounds (default: 64): Miller-Rabin 法で使用するラウンド数（新方式用）。<br><br>
設定ファイルは GUI 実行後に自動的に生成・読み込みされます。<br>
必要に応じて停止後に settings.txt を編集し、再度実行してください。<br><br>

<h2>出力ファイル (primes.txt)</h2>
計算されたすべての素数は primes.txt に行単位で出力されます。<br>
プログラム実行毎に上書きされるため、必要に応じてバックアップを取ってください。<br><br>

<h2>GUI の使用方法</h2>
プログラム起動後に表示されるウィンドウで、下記を行えます：<br><br>

Old Method (Sieve)<br>
prime_min と prime_max に 64 ビット整数で扱える範囲を指定し、「Run (Old Method)」ボタンを押すと範囲内の素数を primes.txt に出力します。<br><br>

New Method (Miller-Rabin)<br>
prime_min と prime_max に任意の正整数を指定し、「Run (Miller-Rabin)」ボタンを押すと Miller-Rabin 法で素数判定を行い、結果を primes.txt に出力します。<br><br>
ログがウィンドウ下部に表示され、計算過程や完了メッセージ、エラーなどを確認できます。<br><br>

<h2>注意事項</h2>
大きな prime_max を指定すると計算に時間がかかり、primes.txt が非常に大きくなる場合があります。<br>
ディスク容量や計算時間には十分ご注意ください。<br><br>

<h2>プロジェクト構成</h2>
sosu-seisei/<br>
├── src/<br>
│   └── main.rs          # メインプログラム（GUIあり）<br>
├── settings.txt         # 設定ファイル（自動生成）<br>
├── primes.txt           # 計算結果出力ファイル<br>
├── Cargo.toml           # プロジェクト設定<br>
├── Cargo.lock           # 依存関係ロックファイル<br>
└── .gitignore           # Gitで無視するファイル等<br><br>

<h2>ライセンス</h2>
このプロジェクトは MIT ライセンス のもと公開されています。詳細は LICENSE ファイルを参照してください。<br>
