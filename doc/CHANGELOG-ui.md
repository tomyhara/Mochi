# UI mocks — change log

対象ファイル: doc/Mochi-UI-mocks.html
対応仕様: doc/ui-spec.md v0.4 (2026-09-12)

## 反映済み

### モック修正 (§3)
- M-1 設定ファイルのパスを OS 標準へ (`~/Library/Application Support/Mochi`)
- M-2 `SOURCE MISSING` → `WORKING DIRECTORY MISSING`、`ARCHIVED` カードを新設、
  `archived` を指していた "source missing" 表記を全廃
- M-3 初回スキャンの進捗数値を整合（**逐次処理**として解釈）
- M-4 ネットワークのトグルを `Fully offline` の静的表示へ
- M-5 `Watching` を監視ストア数に統一し、実行中セッション数を分離

### 新規画面 (§4)
| 節 | 画面 | id |
| --- | --- | --- |
| §4.2 | Main window (Windows) | 3a |
| §4.3 | Integrated terminal（実行中／異常終了／終了確認） | 3b |
| §4.1 | Masking applied（マスク時／解除時／検索ヘッダ） | 3c |
| §4.4 | Delete flow（確認／ゴミ箱不可／選択規則） | 3d |
| §4.5 | Export（既定／マスク OFF／Raw JSONL） | 3e |
| §4.6 | Merge repositories（＋コンテキストメニュー） | 3f |
| §4.7 / §4.7b | Settings → Storage / Cost / About | 3g |
| §4.8 | Subagent transcript | 3h |
| §4.9 | Light theme | 3i |

### §6 チェックリストの状態
モック修正 5 件、新規画面 10 件すべて反映済み。

## 未反映（実装が先行している分）

### M-11 左の選択部を 2 ペインへ（`1b` / `3a` / `3i`）
2026-09-12、実装（`crates/mochi-gui`）で左のツリー型サイドバーを `1a` の 2 ペイン
（リポジトリペイン ＋ セッションペイン）に差し替えた。requirements FR-9.1 / §12.1 は更新済み、
**モックは未追従**。仕様は [ui-spec.md §3 M-11](./ui-spec.md)。

## 未決の 1 件
- **M-3 の処理モデル**: 逐次処理として数値を揃えた（仕様書の修正案どおり）。
  並行処理が正なら、全体値を 2,443 側に合わせる修正が必要。

## 備考
- ライトテーマのアクセントは `#F0968C` では白背景でコントラストが不足するため
  `#B4453A` に差し替えた（NFR-6.2）。diff・警告・実行中インジケータも同様に再調整。
- `3g` の Cost 節の単価は書式を示すための記入例であり、正しい値ではない。
