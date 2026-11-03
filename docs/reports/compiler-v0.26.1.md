# Emela v0.26.1への依存更新

2026-11-03。ユーザー指定の公式releaseへ依存を固定し、Bytesを本体標準へ移管する。
S1〜S6の比較対象と組み込み制約は維持する。

本書はv0.26.1への更新時点の記録である。その後、組み込みmoduleの管理はemelinoへ移した。
`lib/embedded` の再設置と本体 `Std.Embedded.*` への非依存化は
[組み込みライブラリーの所有範囲](embedded-library-ownership.md)を参照する。

## 固定する版

| 項目 | 値 |
| --- | --- |
| release | [v0.26.1](https://github.com/emela-lang/emela/releases/tag/v0.26.1) |
| release commit | `6021115103dd830e0efd9bb96a56a0ca97859329` |
| 旧pin | `ec1031cb0b98386baf0e9b0ced67e8829bb2c529` |
| 更新前のemelino | `f29bafe` |

Cargo.tomlはtag名だけでなくreleaseが指すcommitを指定し、Cargo.lockも一致させる。
syntax・resolve・types・core・lirの5 crateはすべてv0.26.1。他のCargo依存は変更しない。
本体ソースの修正、兄弟repoへのpath依存、別backendの導入は行わない。

## 標準ライブラリーへの移管

`Std.Bytes` はv0.26.0で本体に追加され、今回のreleaseにも含まれる。
本体の `core_sources()` が供給するソース、標準 `Type::Bytes`、8個のBytes builtinを使う。
UTF-8検証・16進変換・Eq/Ord/ShowなどのEmela実装も本体から受け取る。

- `crates/emelino-cli/lib/emelino/bytes.emel` と空になった `lib` を削除。
- `upstream_adapter/bytes.rs` のexternal名・型・signature識別処理を削除。
- Bytes専用の型ID再割当てを撤去。本体と所有IRで同じcanonical IDを使い、型表の整合性を検査する。
- 既存のC runtime・容量検査・所有権処理を再利用する。
- 固定した旧170ソースファイルは変更しない。試験helperでの `Std.Bytes` import書換えを撤去する。

利用側の `import Emelino.Bytes` は `import Std.Bytes` へ変更する。
`Embedded.*` の6互換moduleは引き続き本体 `Std.Embedded.*` から派生させる。
同じ宣言のコピーはrepoへ置かず、6moduleの本体ソースと派生結果のhashを照合する。

## 残る接続処理

`upstream_adapter.rs` は標準Flowをemelino所有のProgramへ投影し、公開入口を選択する境界である。
独立backendと保存形式を維持するため、この投影は必要になる。

旧pinとv0.26.1の `emela-lir/src/ir.rs` は同一。
`from_core.rs` の差分は `Sig::Bytes` の追加だけであり、次の補完は標準Flowに移管されていない。

| 処理 | v0.26.1での扱い |
| --- | --- |
| `upstream_adapter/cause.rs` | Cause.raiseの失敗伝播を既存backendのABIへ接続するため継続 |
| `upstream_adapter/unit_any.rs` | Unit→Any callbackの既知の空String placeholderを正しいUnit boxへ接続するため継続 |
| constの特殊化をまたぐ共有 | 標準Core/Flowは未修正。UPSTREAM-001を継続し、独自の由来追跡は追加しない |

本体の `emela-codegen-emb` にある私的な補完や追加単相化は、標準Flowの公開契約とは別である。
そのbackendを依存に追加して接続部を迂回する変更は行わない。

## 保存済み入力

BackendInputと最小Flow profileのproducer revisionもrelease commitへ更新する。
旧pinの保存入力はrevision不一致として拒否するため、元ソースから生成し直す。
JSON内のrevisionだけを書き換える移行は提供しない。旧revisionの拒否は両形式の既存試験へ追加した。

## 検証

製品・試験コードの検証commitは `fce69bbb87e7c18ca556e099ae16c8ae828a550d`。
その後の対応表・報告更新は製品と試験のコードを変更しない。

| 検証 | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 成功 |
| `cargo test --workspace --locked --offline` | **377成功、失敗・ignore・filterは0** |
| 固定169プログラム・170ソース | 旧原文のhashを保ち、生成成功／具体的wrapperを求める診断が従来どおり |
| 標準Bytesと同名のユーザー型 | builtinのcanonical IDを保持し、ユーザー型を混同しない |
| CLI source削除後の保存入力再生成 | 成功。生成Cをstrict C11のO0/O2で実行 |

全試験の内訳はCLI 237件、compiler 140件でS6と同じ。
廃止したBytes external契約の2試験は、標準の型・builtin投影とユーザー型の区別の2試験へ置換した。
Bytesの所有権・Unicode・traits・同期／非同期C ABI・取消・ISRは既存の観測を維持する。
Cause・Unit/Any・constを含む全試験を実行し、UPSTREAM-001の8観測は既知差分として継続する。
対応表は428定義・226文脈・115代表試験・10要件を保持し、標準Bytesへの移管先を更新した。
対応表確定後に整合試験2件も再実行して成功した。

変更レビューでは、本体5 crate以外の依存変更、compilerへの本体依存、旧Bytes markerの残存が
ないことも確認した。実装と関連試験は日本語の機能別コミットにまとめ、pushは行っていない。

S6の377試験と隔離checkoutの結果は旧pinでの履歴として保持し、新releaseの実績へ読み替えない。
今回の377試験は更新後にworkspaceで再実行した結果である。新たな隔離checkoutでの全試験、
本体JSの再実行、実機boardの動作確認は今回の検証には含めない。
