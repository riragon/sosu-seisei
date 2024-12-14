
![1956](https://github.com/user-attachments/assets/f1cbc201-ccff-4b28-9868-9af1430de862)



<h2>Sosu-Seisei Sieve</h2>
Sosu-Seisei Sieve is a tool designed for efficiently generating prime numbers over large numerical intervals.<br>
Developed in Rust, this GUI application employs <code>eframe</code> (based on <code>egui</code>) to provide a cross-platform graphical user interface.<br>
Parallelization is achieved through <code>rayon</code>, while memory usage monitoring utilizes <code>sysinfo</code>.<br><br>

<h2>Key Features</h2>
- Employs segmented sieving of Eratosthenes to efficiently compute large ranges of prime numbers.<br>
- Users can specify the range via <code>prime_min</code> and <code>prime_max</code> (with a theoretical upper bound of 999999999999999999).<br>
- The <code>split_count</code> parameter allows output files to be divided into multiple parts (with 0 indicating no segmentation).<br>
- Selectable output formats include <code>Text</code>, <code>CSV</code>, and <code>JSON</code>.<br>
- Settings can be modified through the GUI, and execution can be started or interrupted as desired.<br>
- During execution, the progress percentage, estimated time remaining (ETA), and memory usage are displayed.<br>
- Configuration parameters are stored in <code>settings.txt</code> (in TOML format), which is automatically updated upon configuration changes via the GUI.<br><br>

<h2>Directory Structure</h2>
<pre>
sosu-seisei/
├─ Cargo.toml
├─ settings.txt
└─ src/
   ├─ main.rs
   ├─ lib.rs
   ├─ app.rs
   ├─ config.rs
   └─ sieve.rs
</pre>
- <code>Cargo.toml</code>: Defines project dependencies and meta-information.<br>
- <code>settings.txt</code>: The configuration file (TOML format).<br>
- <code>src/main.rs</code>: Entry point for the application (launches the GUI).<br>
- <code>src/lib.rs</code>: Module definitions.<br>
- <code>src/app.rs</code>: Implements the GUI logic, configuration management, and task execution triggers.<br>
- <code>src/config.rs</code>: Handles reading and writing of settings, and defines the <code>Config</code> structure.<br>
- <code>src/sieve.rs</code>: Implements prime number calculations (segmented sieve of Eratosthenes) and parallel processing logic.<br><br>

<h2>Setup and Build</h2>
1. Verify that Rust is installed. If not, please refer to the <a href="https://www.rust-lang.org/ja">official website</a> for installation instructions.<br><br>
2. Navigate to the project directory and execute the following command:<br>
<pre>
cargo build --release
</pre>
Upon successful completion, the binary will be generated in the <code>target/release/</code> directory.<br><br>

<h2>Execution</h2>
<pre>
cargo run --release
</pre>
This command will launch the GUI window.<br><br>

<h2>Regarding the Configuration File (<code>settings.txt</code>)</h2>
<code>settings.txt</code> is in TOML format and is automatically generated upon the first execution.<br>
Subsequent modifications through the GUI will be saved automatically.<br><br>

An example of initial settings (<code>settings.txt</code>):<br>
<pre>
segment_size = 10000000
chunk_size = 16384
writer_buffer_size = 8388608
prime_min = "1"
prime_max = "10000000000"
output_format = "Text"
output_dir = "C:\\Users\\saijo\\Desktop\\素数フォルダー"
split_count = 0
</pre>

<h2>Parameter Descriptions</h2>
- <code>segment_size</code>: The range size for each sieve segment. Larger values increase memory consumption.<br>
- <code>chunk_size</code>: The chunk size employed during processing.<br>
- <code>writer_buffer_size</code>: The buffer size for file writing operations.<br>
- <code>prime_min</code>: The lower bound of the prime range (specified as a string).<br>
- <code>prime_max</code>: The upper bound of the prime range (specified as a string).<br>
- <code>output_format</code>: Select from <code>Text</code>, <code>CSV</code>, or <code>JSON</code>.<br>
- <code>output_dir</code>: The directory path for output files.<br>
- <code>split_count</code>: The number of primes per output file segment (0 indicates no segmentation).<br><br>

<h2>Instructions for Use</h2>
1. After launching the application, specify <code>prime_min</code> and <code>prime_max</code> in the GUI.<br>
2. If necessary, set <code>split_count</code> to segment the output files.<br>
3. Select the desired <code>Output Format</code>.<br>
4. Specify the <code>Output Directory</code> (selectable via the <code>Select Folder</code> button).<br>
5. Once all settings are configured, click the <code>Run</code> button to start processing.<br>
6. During execution, you may click the <code>STOP</code> button to interrupt the process.<br>
7. Check the <code>Log</code> section at the bottom of the interface to review progress and error messages.<br><br>

<h2>License</h2>
This project is provided under the MIT License. Please refer to the <code>LICENSE</code> file for details.<br>
