# 生成Cと手書きCの実行性能比較

同じ入力・結果を持つEmelaと手書きCを、同じCコンパイラ・最適化設定で比較する。
コンパイラの処理時間は計測しない。

```sh
cargo build --locked --offline -p emelino-cli
python3 benchmarks/runtime/run.py
ASAN_OPTIONS=detect_leaks=0 python3 benchmarks/runtime/validate.py \
  --baseline out/runtime-benchmark/results.json
```

`run.py`はCの生成、C11でのコンパイル、結果照合、未解放heap blockの検査、
CPU affinityの固定、実行回数の校正、warmup、各7回の計測を行う。
1 sampleが150ms以上になるよう反復回数を校正し、固定heapは64KiBにする。
生成Cと手書きCの計測順を入れ替え、中央値・MAD・全sampleをJSONへ保存する。
プロセスの起動・ソース生成・Cコンパイル・結果表示は計測区間に含めない。
計測用のCは変更せず、確保・参照操作・配列コピーのカウンターは別の実行ファイルで採る。

```sh
# ケース・最適化・CPU・heap容量・出力先を指定できる。
python3 benchmarks/runtime/run.py --case closure array_pipeline --opt Os O2 \
  --cpu 0 --heap-bytes 65536 --samples 7 --min-ms 150 \
  --output out/runtime-custom/results.json
```

`--emelino`と`--cc`（または`CC`）でツールを選べる。
LinuxのCPU affinityを使用する。今回の検証環境はWSL2 Ubuntu。
生成CのArray容量検査には64bitホストでGCCの`-Wtype-limits`が出るため、
両実装に同じ`-Wno-type-limits`を指定する。他の`-Wall -Wextra -Werror -pedantic`は有効。

`reference.c`は符号なし演算でInt32のwrapを表す。
Listは固定poolの連結リストを3段階作り、Arrayは連続領域を作ってからmap/foldする。
合計を数式へ置き換える処理は書いていない。Cコンパイラが関数内で行う最適化は許す。
C側のList/Array入力上限は4096要素、現在の計測サイズは256要素。
所有権管理と表現の費用も比較対象であり、C側に汎用の参照カウントを追加してはいない。

前後それぞれのコンパイラで別ディレクトリへ計測した場合は、保存した生成Cを再コンパイルして交互に比較できる。

```sh
python3 benchmarks/runtime/compare.py --before out/runtime-before/results.json \
  --after out/runtime-after/results.json --case closure
```

`compare.py`は入力・生成Cのhash、Cコンパイラ、最適化設定、heap容量などを照合する。
同じベンチマーク入力に対するコンパイラ変更の比較に使用する。

`experiments.py`は初回調査時の生成Cにケース限定の変更を加え、変更前後を再計測する。
コンパイラの最適化パスではなく、汎用化したときの正しさを証明するものでもない。
元Cのhashと置換箇所を検査し、生成形式が変わった場合は停止する。
元の生成Cは保存し、変更したCは`out/.../build/`内に別名で置く。
最適化実装後の生成Cには一致しないため、その場合は`compare.py`で実装前後を比較する。

`validate.py`は生成Cと存在する実験CにASan/UBSanを適用する。
この実行環境ではptraceの影響でLeakSanitizerが起動できないため、上のコマンドで無効にする。
固定heapの回収は`run.py`と`experiments.py`の別カウンターで検査する。

初回の結果・提案は[2026-11-12の報告](../../docs/reports/runtime-performance-2026-11-12.md)、
生データは[results](results/)を参照。
保存済みJSONの`executable_size`は両実装とharnessを含む実行ファイル全体の参考値であり、
生成物同士のサイズ比較ではない。マイコン上の実行速度はこのホスト計測とは別に測る。
