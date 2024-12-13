<h2>概要</h2>
Sosu-Seisei は、指定した範囲内の素数を効率的に生成するツールです。<br>
本ツールは GUI を通じて、64ビット整数範囲でのセグメント化エラトステネスの篩を用いた旧方式（Sieve 法）による素数列挙を行います。<br>
<code>prime_min</code> と <code>prime_max</code> を入力し、「Run (Old Method)」ボタンを押せば、指定範囲内の素数が <code>primes.txt</code> に出力されます。<br><br>

<h2>方式 (Sieve 法)</h2>
<code>prime_min</code> から <code>prime_max</code> までの範囲を、小さなセグメントに分割してエラトステネスの篩を適用する「セグメント化篩」により、高速かつメモリ効率よく素数を求めます。<br>
<code>bitvec</code> クレートによるビット単位の管理でメモリ使用量を低減しています。<br><br>

<h2>使用クレート</h2>
<ul>
  <li><b>rayon</b>: 並列処理で篩計算を高速化</li>
  <li><b>bitvec</b>: ビットベクトルでメモリ使用量を削減</li>
  <li><b>serde + toml</b>: 設定ファイル (<code>settings.txt</code>) の読み書き</li>
  <li><b>egui + eframe</b>: GUI 構築用フレームワーク</li>
  <li><b>sysinfo</b>: システム情報取得（メモリ使用量など）</li>
</ul>
<br>

<h2>インストールと実行</h2>
1. リポジトリをクローン:<br>
<code>git clone https://github.com/riragon/sosu-seisei.git</code><br>
<code>cd sosu-seisei</code><br><br>

2. リリースビルドで実行:<br>
<code>cargo run --release</code><br><br>
起動後に GUI ウィンドウが表示されます。<code>settings.txt</code> が存在しない場合は自動生成されます。<br><br>

<h2>設定ファイル (settings.txt)</h2>
<code>settings.txt</code> は TOML 形式で、セグメントサイズや範囲 (<code>prime_min</code>, <code>prime_max</code>) などを指定可能。<br>
初回起動時に自動生成され、停止後に編集した値は次回起動時に反映されます。<br><br>

<h2>出力ファイル (primes.txt)</h2>
「Run (Old Method)」実行後、求めた素数が行ごとに <code>primes.txt</code> に出力されます。<br>
同名ファイルは上書きされるため、必要ならバックアップを取得してください。<br><br>

<h2>GUI の使用方法</h2>
1. <code>prime_min</code> (下限) と <code>prime_max</code> (上限) を入力<br>
2. 「Run (Old Method)」をクリックで計算開始<br>
3. 計算完了後、<code>primes.txt</code> に素数一覧が出力されます。<br>
処理状況やログはウィンドウ下部に表示されます。<br><br>

<h2>注意事項</h2>
- 非常に大きな <code>prime_max</code> を指定すると、計算に時間やメモリが多く必要になります。<br>
- 環境に応じて範囲やセグメントサイズを適宜調整してください。<br><br>

<h2>プロジェクト構成</h2>
<pre>
sosu-seisei/
├─ src/
│  └─ main.rs         # メインプログラム (GUI)
├─ settings.txt       # 設定ファイル (自動生成)
├─ primes.txt         # 結果出力ファイル
├─ Cargo.toml         # プロジェクト設定
└─ Cargo.lock         # 依存関係ロックファイル
</pre>
<br>

<h2>ライセンス</h2>
このプロジェクトは MIT ライセンスで公開されています。<br>
詳細は <code>LICENSE</code> ファイルを参照してください。<br>
