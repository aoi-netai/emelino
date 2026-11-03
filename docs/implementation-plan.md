# 実装範囲と検証の終了条件

emelinoはEmela v0.26.1の公開APIから受けたFlowを、組み込み向けCへ変換する。
機能ごとの意味と境界は[構成・機能契約](architecture.md)に定義する。
本書は採用範囲、機能の組立順、検証方針の正本とし、過去のS段階の報告を現在の仕様の代わりにしない。

## 責任と対象範囲

対応目標は最終確認PRで同梱する固定Flow対応表の対象機能を組み込み制約下で扱うことであり、固定本体v0.26.1のC targetで検証する。
全Emela機能や全targetへの保証ではない。固定台帳を対応範囲の正本とする。

| 所有者 | 責任 |
| --- | --- |
| 固定版本体 | parse/resolve/check、単相化、標準Flow、汎用core/std、Std.Bytes |
| emelino CLI | 明示source入力、公開入口設定、本体API接続、Flowから所有IRへの投影、保存入力と設定の読書き |
| emelino compiler | 保存入力契約、lower/plan/emit、所有権、固定heap/pool、failure/取消、C ABI、ISR制約 |
| emelinoの組み込み層 | Embedded.*、board header、C adapter、生成物manifest |
| 後続作業 | Pome/driver連携、物理BSP、startup/vector/linker、firmware build/flash、実機検証 |

本体5 crateは`6021115103dd830e0efd9bb96a56a0ca97859329`へ固定し、Cargo.lockと揃える。
兄弟repoへのpath依存、独自frontend、汎用core/stdの複製、本体の別codegenへの依存を追加しない。
本体をpatchせず、raw Flowに対するclosure・handler・suspend変換はbackendで一度だけ行う。
生成Cは最適化前に意味が成立することを要求する。

## 機能の組立順

本文のPR1〜PR8は機能の実装順序番号であり、GitHub上のPR番号とは区別する。
実際のPR番号・base・依存関係は公開したstackと各PRリンクを参照する。

各PRは直前のPRを前提とする。表は納入範囲であり、ここに記載しただけで試験成功とはしない。

| PR | 機能 | 判定する観測 |
| --- | --- | --- |
| 1 | v0.26.1基盤、公開入口、保存入力、基本制御 | sourceからC実行、source削除後の再生成、引数/call/if、不正型・ID・集合・設定の拒否 |
| 2 | データ表現、固定heap、所有権 | record/enum/List/Any/再帰、更新順、寿命とdeep release、枯渇 |
| 3 | 数値、String、Unicode、標準Bytes | 境界値・bit値、NUL/alias/容量、scalar/grapheme、UTF-8/u32長/unsigned値 |
| 4 | closure、effect/State、failure/Cause、Unit/Any | capture、handler伝播、cleanup、所有error、defect停止、callback表現 |
| 5 | async task、Scope/Fiber/Deferred | 中断/再開順、複数waiter、取消、slot再利用、固定pool、所有値の回収 |
| 6 | constとUPSTREAM-001 | 単相constの初期化/cache/defect/所有、汎化constの影響観測を分離 |
| 7 | C ABI、ISR、Embedded.*、board hook | 所有adapter、完了/取消、生成C名衝突、ISR到達性、module供給と上書き拒否 |
| 8 | 対応表、出典、全体の再現検証 | 全キーと根拠、機能commit対応、workspaceと独立archive、ソース不要再生成 |

## 検証の方針

source機能は**ソース → 本体frontend → Flow → backend → C実行**の観測を必須とする。
sourceと保存済み入力の再生成でC/headerを比較し、対象Cをstrict C11の`-O0`/`-O2`で実行する。
C compiler不在、skip、コンパイルのみ、artifactだけの確認を実行成功に数えない。
IR単体・入力拒否・schema検証・source後の試験専用IR加工はそれぞれ別の証拠にする。
生成C本文の旧実装とのbyte一致は機能条件にせず、値・副作用順・failure・所有権・資源を比較する。

固定heap/poolを含む試験は容量も条件の一部として記録する。
TestClockの補足観測はframe_bytesを1024から2048へ変更した条件を持ち、
短絡評価・引数順序のeffect/handler観測はheapを0から4096へ変更した条件を持つ。
それぞれのsource・設定・harnessをrepo内で保持し、旧容量で成功したとは扱わない。
試験用のheap観測関数やconst初期化計数hookは製品ABI・通常sourceの副作用契約から分ける。

fmt、strict all-target clippy、workspace全試験と必要な独立再実行を行う。
実行記録には実際のcommit、製品/試験tree、コマンド、環境、件数、終了コード、log hashを記載する。
文書だけの後続commitは、検証した製品/試験treeとの一致を確認してから結果を参照する。
過去の成功件数やSHAを、新しいcommitで実行した結果へ読み替えない。

## 終了条件と残る境界

各対象の入力、意味、担当実装、試験、実行記録が辿れ、理由不明の未判定・欠落・重複がないことを求める。
`verified`、理由付き`not_applicable`、明示した`upstream_known_issue`を区別する。
本体への担当変更でも必要なsourceの観測は残し、未実装や試験困難を適用外に置き換えない。
数や候補pathの一致だけで意味を検証済みにせず、文脈ごとの観測を照合する。

UPSTREAM-001は型特殊化をまたぐconst共有の未修正差分である。
影響する8件の観測を分け、受信Flowのconstごとの初期化・cache・defect・所有は通常どおり検証する。
この例外を新たな差分の自動承認や、任意の入力に対する正しさの証明に使わない。
汎用のroot指定API、本体const修正、project driver、実機接続は、それぞれ契約と試験を確認する後続変更とする。
