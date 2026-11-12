# 局所クロージャの確保除去（2026-11-12）

実装commitは`08716b0`、ブランチは`perf/non-escaping-closures`。
初回調査で最も大きな差があったクロージャ生成・呼び出しを最適化した。
生成Cの限定的な書換えではなく、ソースと保存済みBackendInputの両方へ適用するbackend loweringである。

## 実装した変換

同じ関数内で直接呼ぶ用途だけに使われる、同期・infallibleなクロージャを対象にする。
captureはInt・Int64・Float・Boolに限定する。
生成時点の値を新しいscalar変数へsnapshotし、liftした関数の通常引数として渡す。
関数値と環境を作らず直接callするため、heap確保、参照操作、間接callを除ける。

`adder(n)`のように、引数をcaptureしたクロージャだけを返す純粋なfactoryも、
局所的な呼び出しへ展開してから同じ変換を適用する。
複製によるコード増加を抑えるため、factoryは入れ子の本体も含めて16 statement以下に限定する。
関数引数の評価順を変えず、factoryの本体に別の処理がある場合は展開しない。

別名への代入、aggregateへの保存、返却、高階関数・externalへの引数渡し、複数定義、
管理対象のcapture、failure・suspendを持つ場合は従来の所有するクロージャを使用する。
const初期化や、関数引数で表す自己末尾再帰を持つクロージャも今回の変換から除外する。
公開C ABIとBackendInputのschemaは変更していない。

実装は[`closure/local.rs`](../../crates/emelino-compiler/src/backend/lower/closure/local.rs)。
既存のclosure body remapを再利用し、closureのlift時に適用する。
const特殊化で使う後続closure familyのordinalは、確保を除いた場合も維持する。

## 実装前後の再計測

初回調査の生成Cが保存したSHA-256と一致することを確認し、実装後の生成Cと同じGCCで再コンパイルした。
前後の計測順を入れ替え、CPU 0、各7 sample、150msを目標に校正した反復回数で測定した。
環境は前回と同じRyzen 9 3900X / WSL2 / GCC 11.4.0、固定heapは64KiB、LTOなし。

クロージャを1万回作って呼ぶ1回の処理に対する中央値:

| 設定 | 実装前 | 実装後 | 高速化 |
| --- | ---: | ---: | ---: |
| `-Os` | 403.760µs | 2.414µs | 167.24倍 |
| `-O2` | 329.662µs | 2.418µs | 136.35倍 |

確保回数は10,000 → 0、解放操作も10,000 → 0となった。
別途行った全8ケースの手書きC比較では、最適化後の同ケースは以下の結果だった。

| 設定 | 最適化後の生成C | 手書きC | 生成C / C |
| --- | ---: | ---: | ---: |
| `-Os` | 2.475µs | 2.475µs | 1.00倍 |
| `-O2` | 2.634µs | 2.672µs | 0.99倍 |

別の時間帯の測定なので絶対時間には揺れがあるが、手書きCとの大きな差は解消した。
環境の確保を除くことでCコンパイラが呼び出し全体を簡約できる効果も含む。

数値ループ・Fibonacci・List pipeline・Array pipeline・Array走査・Array更新の6ケースは、
実装前後で生成CのSHA-256が完全一致した。
Stateでは局所的なクロージャが1つ除かれ、確保が7 → 6になった。
Stateの前後交互計測は`-Os`で241.424 → 246.731µs、`-O2`で137.064 → 138.841µsで、
大きな速度改善はない。handlerや参照カウントの最適化は今回の対象に含めていない。

## Arm向けオブジェクトの確認

Arm GNU GCC 10.3.1で、前後の生成CをCortex-M0+ / Cortex-M4向けにクロスコンパイルした。
以下は`arm-none-eabi-size`のオブジェクト全体に対するtext / bss列である。

| CPU | 設定 | text 前 → 後 | bss 前 → 後 |
| --- | --- | ---: | ---: |
| Cortex-M0+ | `-Os` | 813 → 16 byte | 65,668 → 0 byte |
| Cortex-M0+ | `-O2` | 964 → 24 byte | 65,668 → 0 byte |
| Cortex-M4 | `-Os` | 799 → 16 byte | 65,668 → 0 byte |
| Cortex-M4 | `-O2` | 928 → 20 byte | 65,668 → 0 byte |

このモジュールではheap全体が不要になり、設定した64KiBのheapと管理領域が消える。
オブジェクトの集計であり、最終リンク後のファームウェアサイズやマイコン実行時間の測定ではない。

## 検証

- 既存workspaceテストに成功。
  最終の同期条件・family ordinalの調整後にはcompilerの129テストと、
  新規4テスト・const関連36テストを再実行して成功した。
- 新規[`local_closures`](../../crates/emelino-cli/tests/local_closures.rs)は、
  heap容量0でのfactory・混在scalar・入れ子の反復実行、capture元を後で再定義する保存IR、
  escape・管理対象capture・failureの所有権回収をO0/O2で確認する。保存入力の再生成一致も検査する。
- `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`、
  `cargo fmt --all --check`、`git diff --check`に成功。
- 全8ケース × Os/O2の2,560結果の照合に成功。未解放heap blockは全ケース0。
- 最適化後の8ケースをASan/UBSanで別途コンパイルし、計384結果の照合に成功。
  ptrace環境で起動しないLeakSanitizerは無効化し、固定heapの回収は別のカウンターで確認した。

再計測は[`compare.py`](../../benchmarks/runtime/compare.py)で実行できる。
実装前（`020f09c`）と実装後（`08716b0`）それぞれのCLIで、
`run.py --output out/runtime-before/results.json` / `out/runtime-after/results.json`を実行し、
両方の生成Cを保存してから比較する。

```sh
python3 benchmarks/runtime/compare.py --before out/runtime-before/results.json \
  --after out/runtime-after/results.json --case closure state
```

生データ: [実装後の全ケース](../../benchmarks/runtime/results/2026-11-12-local-closures-after.json)、
[前後の交互計測](../../benchmarks/runtime/results/2026-11-12-local-closures-comparison.json)、
[Armオブジェクト](../../benchmarks/runtime/results/2026-11-12-local-closures-arm-size.json)、
[ASan/UBSan](../../benchmarks/runtime/results/2026-11-12-local-closures-sanitizers.json)、
[検証条件・入力hash](../../benchmarks/runtime/results/2026-11-12-local-closures-validation.json)。
