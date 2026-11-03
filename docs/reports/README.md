# 検証・出典・履歴記録の読み方

現在の仕様は[構成・機能契約](../architecture.md)、対応は[現在の台帳](../compiler-coverage.md)を読む。
このdirectoryの既存S報告、採用記録、JSON evidenceは、当時のrevisionと条件を保った履歴である。
「現在」「今回」「完了」と記された箇所も、その報告内のcommitを対象とする。
再編後の実行は[再現検証記録](reconstruction-validation.json)で別に確定する。

| 資料 | 用途 |
| --- | --- |
| [compiler-s1](compiler-s1.md)〜[compiler-s6](compiler-s6.md) | 旧pinで段階的に接続した内容と当時の実行結果 |
| [compiler-s6-validation.json](compiler-s6-validation.json) | 旧pinのworkspace・独立archive実行。新revisionの成功には読み替えない |
| [compiler-v0.26.1](compiler-v0.26.1.md) | 本体release固定と標準Bytes移行時の結果 |
| [embedded-library-ownership](embedded-library-ownership.md) | Embedded.*をローカル管理へ移した結果 |
| [spec-boundary-fixes](spec-boundary-fixes.md) | 不正FnSets・adapter名衝突の修正と386試験の旧実行記録 |
| [reuse-provenance](reuse-provenance.md)・`reuse-*.json` | 採用元、fixture変更、元hash、容量条件、観測対応 |
| [backend-coverage-review](backend-coverage-review.json)・[reuse-source-record-review](reuse-source-record-review.json) | schema/IR/sourceの意味と担当範囲を照合した根拠 |
| [history-crosswalk](history-crosswalk.json) | 元60commitから新機能PR・commitへの案内 |

## 旧SHAと再編後の履歴

再編前のemelino履歴はannotated tag `archive/pre-history-cleanup`で保全する。
作業用の保全branch名は`archive/original-implementation`である。
旧報告に記載されたemelinoのSHAはこの履歴の実物を指し、新commitのSHAで置換しない。
新しい履歴はbootstrap mainから8機能PRへ分け、最終製品/試験ファイルを元
`948a8be7e3bb01956b1558aa42204d19f177bb59`と比較する。

```sh
git show archive/pre-history-cleanup:docs/reports/spec-boundary-fixes.md
git show ced5789af9b06538ab49bda88f12d2cf446bcae3
```

上記は同repoの旧emelino資料を読む操作である。
別repoのemela-emb・旧fork・本体のSHAは、同じtagで解決するとは限らない。
それらは[再利用元](reuse-provenance.md)と[固定表のprovenance](../reference/legacy-flow/provenance.json)に
repo名、元path、ライセンス、hashを保持する。
歴史資料の公開済み/未公開の記述は採取時点の状態であり、外部URLの現在の到達性を保証しない。

## 保存する事実と新しい判定

source corpusのID・原文・C assertion・hash、固定表のID・旧path・旧symbol、既存実行SHAを保持する。
一つの旧commitが複数の機能PRへ分割された場合や、旧方式が最終仕様で置換された場合は
対照表の配列と理由に記す。機械的な一対一cherry-pickの記録ではない。
new revision欄が未確定の対照表と`pending`の再現結果は、最終確定前に実物から補完する。

本体constの旧Node実行、S6の旧pin隔離実行、後続v0.26.1のworkspace実行は別の観測である。
ネットワーク新規取得、straceによる非アクセス証明、実機board実行は、実施した記録がない限り成功にしない。
Apache-2.0のLICENSEと各採用元の既存表示を維持し、出典資料を現在のAPI契約へ無言で書き換えない。
