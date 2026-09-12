# Mochi — UI 詳細仕様

| 項目 | 内容 |
| --- | --- |
| ドキュメント種別 | UI 仕様（モック修正指示＋未作成画面の仕様） |
| バージョン | 0.4 |
| 最終更新 | 2026-09-12 |
| 親ドキュメント | [requirements.md](./requirements.md)（§12） |
| 対象モック | [Mochi-UI-mocks.html](./Mochi-UI-mocks.html)（変更履歴: [CHANGELOG-ui.md](./CHANGELOG-ui.md)） |

## 0. このドキュメントの位置づけ

既存モックへの**修正指示**（§3）と、**未作成画面の仕様**（§4）を、デザイナ／実装者が迷わない粒度で定めたもの。

- UI 文字列はすべて**英語**（requirements NS-6）。本書中のコードブロック内が実際の表示文字列。
- レイアウトは `1b`「Transcript first」を前提とする（requirements FR-9.1）。ただし **左の選択部は `1a` の 2 ペイン構成**（リポジトリペイン＋セッションペイン）に差し替え済み（2026-09-12、requirements §12.1）。本書で「サイドバー」と書かれた箇所は、リポジトリペインとセッションペインの両方を指す。
- 本書は**画面の意味と文言**を定める。色・余白・タイポグラフィは既存モックの体系に従い、ここでは規定しない。

---

## 1. 共通ルール

### 1.1 セッション状態の正典

モックで用語が混線していたため、**以下を唯一の正典とする**（requirements FR-2.11 / FR-7.6）。

| 内部値 | 表示ラベル | 意味 | 閲覧 | 再開 | 削除 |
| --- | --- | --- | :-: | :-: | :-: |
| `running` | `Running` | 実行中。内蔵ターミナル（FR-7.8e）またはファイル監視で検出 | ✅ | — | ❌ |
| `ok` | （表示なし） | 正常 | ✅ | ✅ | ✅ |
| `partial` | `Partially read` | 末尾が不完全（追記中など）。読めた範囲を表示 | ✅ | ✅ | ✅ |
| `read_failed` | `Read failed` | パース不能 | ❌ | ❌ | ✅ |
| `archived` | `Archived` | **トランスクリプトの元ファイルが消えた**。本文はインデックスに残る | ✅ | ❌ | — |
| `cwd_missing` | `Working directory missing` | **セッションの cwd が消えた**。ファイルは健在 | ✅ | ❌ | ✅ |

**重要**: `archived` と `cwd_missing` は**独立した軸**であり、同時に成立しうる（ファイルも cwd も消えた）。
その場合は `Archived` を優先表示し、詳細に両方を示す。**"Source missing" という語は今後使わない。**

### 1.2 パスとキーボードの OS 別表記

| 対象 | Windows | macOS |
| --- | --- | --- |
| パス区切り | `C:\Users\tomy\code\mochi` | `~/code/mochi` |
| ホーム省略 | 省略しない（`C:\Users\<name>\...`） | `~` に短縮 |
| 修飾キー | `Ctrl` / `Alt` / `Shift` | `⌘` / `⌥` / `⇧` |
| 確定キー表記 | `Enter` | `⏎` |
| 取消キー表記 | `Esc` | `⎋` |
| ゴミ箱の呼称 | `Recycle Bin` | `Trash` |

| ファイルマネージャで表示 | `Show in Explorer` | `Reveal in Finder` |

**実行環境の OS で自動的に切り替える。** モック上は両方のアートボードを用意する（§4.2）。

#### ★ 切り替えの対象は「Mochi 自身が表示するファイルシステムパス」に限る

上の規則が及ぶのは、サイドバーのリポジトリパス・右レールの作業ディレクトリ・元ファイルパス・
設定画面のパスなど、**Mochi がファイルシステムから得て自分で描画するパス**だけである。

**トランスクリプト本文に含まれるパスは対象外。** 会話・ツール呼び出し・diff ヘッダ・シェル出力に
現れるパスは CLI が記録したデータであり、**一切書き換えずそのまま表示する**
（requirements FR-2.5 / NFR-3.5 の「元データを改変しない」は表示にも及ぶ）。
git のリポジトリ相対パスは Windows でも `/` 区切りが慣例なので、diff ヘッダが
`src/repo/identity.rs` のまま Windows 画面に出るのが正しい。

#### 右レールのファイル操作

| 操作 | 表示 | 位置 |
| --- | --- | --- |
| エディタで開く（FR-5.10） | `Open in editor` — **両 OS で同一** | 右レール下部の主ボタン |
| ファイルマネージャで表示 | `Show in Explorer` / `Reveal in Finder` | 元ファイルパスの右クリック、および `⋯` メニュー |

主ボタンを OS で別の動作に差し替えてはならない。「エディタで開く」と「場所を表示する」は別の操作である。

### 1.3 トーン

- ボタンは動詞から始める（`Rescan now`, `Move to Trash`）。
- エラーは「何が起きたか」→「どこで」→「次にどうするか」の順（既存モックの `2d` の作りを踏襲）。
- 破壊的操作のボタンだけは、行われることをそのまま書く（`Move to Trash` であって `OK` ではない）。

---

## 2. 画面インベントリ

2026-09-10 の更新（[CHANGELOG-ui.md](./CHANGELOG-ui.md)）で **全画面が作成済み**。合計 32 画面。

| # | 画面 | 状態 | 節 |
| --- | --- | --- | --- |
| 1a | Main window（3 ペイン固定） | 左 2 ペイン（リポジトリ／セッション）のみ採用 | — |
| 1b | Main window（macOS）— **基準レイアウト** | ✅ 作成済み。⚠️ M-9 で派生版に追従が必要、⚠️ M-11 で左 2 ペインに追従が必要 | — |
| 1c | Command palette / Resume dialog | ✅ | — |
| 2a | Settings | ✅ M-1, M-4 反映済み | §3 |
| 2b | Search results | ✅ `Masking on` ヘッダ追加済み | §4.1 |
| 2c | First run | ✅ M-3 反映済み（逐次処理） | §3 |
| 2d | Empty & error states | ✅ M-2 反映済み（`Archived` / `Working directory missing` に分離） | §3 |
| 3a | Main window（Windows） | ✅ ⚠️ M-7, M-8, M-11 | §4.2 |
| 3b | Integrated terminal（実行中／異常終了／終了確認） | ✅ ⚠️ M-6 | §4.3 |
| 3c | Masked transcript（マスク時／解除時） | ✅ | §4.1 |
| 3d | Delete flow（確認／ゴミ箱不可／選択規則） | ✅ | §4.4 |
| 3e | Export（既定／マスク OFF／Raw JSONL） | ✅ | §4.5 |
| 3f | Merge repositories（＋コンテキストメニュー） | ✅ | §4.6 |
| 3g | Settings → Storage / Cost / About | ✅ | §4.7, §4.7b |
| 3h | Subagent transcript | ✅ | §4.8 |
| 3i | Light theme | ✅ アクセント `#B4453A`（5.45:1）⚠️ M-8, M-11 | §4.9 |

---

## 3. 既存モックの修正指示

### 第 1 ラウンド（M-1 〜 M-5）— ✅ 反映済み

2026-09-10 の更新ですべて反映されたことを確認済み（描画と文言の機械検査）。以下は記録として残す。

### M-1 · 設定ファイルのパス（`2a` フッタ）

XDG（Linux）の慣習になっており、対象 OS と合わない。requirements FR-10.1。

```
現在:  config.toml · ~/.config/mochi

修正後（macOS 版アートボード）:
       config.toml · ~/Library/Application Support/Mochi

修正後（Windows 版アートボード）:
       config.toml · %APPDATA%\Mochi
```

### M-2 · `SOURCE MISSING` カードの改名（`2d`）

本文は「作業ディレクトリが消えた」だが、ラベルの `source missing` は他の 3 箇所で「トランスクリプトファイルが消えた」の意味で使われている。§1.1 に従い分離する。

```
現在:
  ラベル   SOURCE MISSING
  見出し   The working directory is gone
  本文     The transcript is still readable, but resuming needs a
           directory that exists. Point it at the new location to keep
           the history attached.
  パス     ~/old/scratch-notes
  ボタン   [Relocate…] [Keep read-only]

修正後:
  ラベル   WORKING DIRECTORY MISSING        ← ここだけ変更
  （見出し・本文・パス・ボタンはそのまま。本文の内容はこのラベルで正しい）
```

あわせて、**`archived` を指している既存の表記からも "source missing" を外す**。

```
サイドバー（1a）:  Archived (source missing)   31   →   Archived   31
セッション行:      [source missing] … kept in index →  [archived] … kept in index
初回スキャンログ:  31 sessions kept with source missing
                                    →  31 sessions kept after their files were removed
```

さらに、**`archived` 用のエラーカードを新規に追加**する（現在このカードが存在しない）。

```
  ラベル   ARCHIVED
  見出し   The transcript file is gone
  本文     Mochi kept the conversation in its index, so you can still
           read and search it. Resuming is not possible — the CLI needs
           the original file.
  パス     ~/.claude/projects/-Users-tomy-code-mochi/01J9F7Q3.jsonl
  補足     File missing since Sep 8 · Claude Code removes transcripts
           after 30 days by default
  ボタン   [Export…] [Remove from index]

  ※ 補足行は「いつから無いか」（Mochi が観測した事実）と「よくある原因」を並べる。
    "Removed by Claude Code's retention" のように**原因を断定しない**こと —
    Mochi はファイルが消えたことしか観測できず、手動削除や別ツールの整理と区別できない。
```

### M-3 · 初回スキャンの進捗が矛盾（`2c`）

```
現在:
  全体      1,204 of 3,182 sessions · 38%
  Codex          ✓ 2,041 indexed        ← 完了しているのに
  Claude Code    reading 402 / 1,032    ← 合計 2,443 で全体の 1,204 を超える
  OpenCode       queued · 109

修正案（全体を 1,204 に合わせる。Codex を進行中にする）:
  全体      1,204 of 3,182 sessions · 38%
  Codex          reading 1,204 / 2,041
  Claude Code    queued · 1,032
  OpenCode       queued · 109
```

「順に処理する」設計なら上記。「並行処理する」設計なら全体の数を 2,443 側に合わせる。**どちらでもよいが、モックの数字は実装の仕様として読まれるため整合させること。**

### M-4 · ネットワークの表現（`2a`）

```
現在:  Send anything over the network        [トグル・OFF]
       Off, and there is no code path that would
```

切り替え部品は「ON にできる」と読める。requirements NFR-3.1 の主張と矛盾するため、**静的な表示に変える**。

```
修正後:  ● Fully offline
         Mochi has no network code path. Nothing about your repositories,
         sessions or paths leaves this machine.
```

### M-5 · `Watching` の語の統一

`1a` ヘッダの `Watching · 2 running`（実行中セッション数）と `1b` フッタの `Watching 3 stores`（監視中ストア数）で対象が違う。`1b` 採用に伴い後者に統一し、実行中セッション数は別の場所に出す。

```
フッタ左:   ● Watching 3 stores · last scan 8s ago     ← 維持
フッタ右:   2 sessions running                          ← 追加（内蔵ターミナル込み）
```

### 第 2 ラウンド（M-6 〜 M-11）— 更新後モックのレビューと実装で発見

いずれも軽微。実装着手前に反映すればよい。

#### M-6 · ターミナルの `$` プロンプト（`3b`）

```
現在:   $ codex resume 01J9F7Q3X2ZK4M8C2A
修正後: ▸ codex resume 01J9F7Q3X2ZK4M8C2A
```

同じ画面の SCOPE パネルが "There is no shell prompt" と述べているのに、起動コマンド行の `$` は
まさにシェルプロンプトの記号で、画面内で矛盾する。起動コマンドの**エコー**であることが分かる別の記号にする。

#### M-7 · diff ヘッダのバックスラッシュ（`3a`）

```
現在:   src\repo\identity.rs
修正後: src/repo/identity.rs
```

§1.2 の「切り替えの対象は Mochi 自身が表示するパスに限る」を参照。diff ヘッダはトランスクリプト本文
（CLI が記録したデータ）であり書き換えない。**この規則は §1.2 に書かれていなかった仕様側の漏れ**で、
モック作者の判断は当時の仕様に忠実だった。

#### M-8 · 右レール主ボタンが OS で別の動作になっている（`3a`, `3i`）

```
1b (macOS)     Open in editor
3a (Windows)   Show in Explorer     ← 別の操作に置き換わっている
3i (light)     Reveal in Finder     ← 同上

修正後: 3 画面とも主ボタンは  Open in editor
        Show in Explorer / Reveal in Finder は元ファイルパスの右クリックへ
```

§1.2「右レールのファイル操作」を参照。

#### M-9 · 基準レイアウト `1b` が派生版より古い

`3a`（Windows）と `3i`（ライト）は `1b` の派生だが、`1b` にない要素を持つ。

| 要素 | 1b | 3a | 3i |
| --- | :-: | :-: | :-: |
| `● Terminal` タブ | ❌ | ✅ | ✅ |
| 右レール `Est. cost` 行 | ❌ | ✅ | ✅ |

`1b` は実装の基準（requirements FR-9.1）なので、**`1b` に両方を追加**して 3 画面を揃える。
あわせて `Est. cost` の値には `Estimate from your pricing.toml — not your bill` をツールチップで付ける
（requirements FR-10.7 は「表示箇所ごとに明示」を求めている。Settings → Cost の注記だけでは足りない）。

#### M-10 · `ARCHIVED` カードの原因断定

```
現在:   Removed by Claude Code's 30-day retention on Sep 8
修正後: File missing since Sep 8 · Claude Code removes transcripts after 30 days by default
```

Mochi が観測できるのは「ファイルが無い」ことだけで、消えた原因（保持期間・手動削除・別ツール）は
区別できない。事実と「よくある原因」を分けて書く。**元の文言は本書 §3 M-2 で私が示した例文**であり、
モック側の誤りではない。

#### M-11 · 左の選択部が 2 ペインになった（`1b`, `3a`, `3i`）

実装（`crates/mochi-gui`）で、左のツリー型サイドバーを `1a` の 2 ペインに差し替えた
（requirements FR-9.1 / §12.1、2026-09-12）。理由は、リポジトリ見出しとセッション行が
1 本のリストに並ぶため、**どの行が「選ぶ対象」でどの行が「開く対象」なのかが画面から読み取れない**こと。
モック側もこれに追従する。

| | 現在のモック（`1b` 系） | 修正後 |
| --- | --- | --- |
| 左 | ツリー 1 枚（リポジトリ配下にセッションを展開、`Show N more`） | **2 ペイン**: リポジトリペイン ＋ セッションペイン |
| リポジトリペイン | — | 見出し `Repositories` ＋ 件数、`Filter repositories`、`All repositories` 行、リポジトリ行（名前・件数・パス・`cdx 12 · cc 8 · 3h ago`）、末尾に `No repository` / `Archived`（0 件なら出さない） |
| セッションペイン | — | 見出し＝選択中のリポジトリ名＋パス＋`N sessions`、`Filter sessions`、`TODAY` / `YESTERDAY` / `PREVIOUS 7 DAYS` / `PREVIOUS 30 DAYS` / `OLDER` / `NO DATE` の見出しで区切った行（1 行目＝ツールタグ・タイトル・警告、2 行目＝時刻 · `N msgs` · ブランチ） |
| 並び | リポジトリは名前順 | リポジトリは**最終更新の新しい順**（0 件のものは末尾、名前順） |

`1a` のアートボードは幅 `264px / 372px / 1fr`。実装の既定値は `236px / 324px / 可変 / 330px`（右レールは `1b` のまま）。
文言は上表が正典とし、`3a`（Windows）・`3i`（ライト）も同時に差し替える。

---

## 4. 未作成画面の仕様

> **2026-09-10: 本節の全画面が作成済み**（§2 参照）。以下は各画面の仕様として引き続き有効。

### 4.1 マスキング適用後のトランスクリプト 〔優先: 高〕

**対応要件**: NFR-3.3 ／ **理由**: 切替部品は 3 箇所にあるが、適用後の見た目という肝心の部分が未定。

#### 考え方

セッションログには `.env` の内容や API キーがそのまま載る。**既定は「隠す」**。ただし
「全部隠して読めない」では価値がないため、**行単位ではなく値単位で伏せ、種別を示す**。

#### 表示規則

| 検出対象 | 表示 |
| --- | --- |
| API キー・トークン（`sk-`, `ghp_`, AWS, `Bearer` 等の既知パターン） | `sk-••••••••••••••••` — 先頭 3 文字＋伏字。長さは実際の長さを反映しない |
| `.env` ファイルの読み取り結果 | 値のみ伏せ、キー名は残す（`DATABASE_URL=••••••••`） |
| 高エントロピー文字列（既知パターン外） | `[possible secret]` — 誤検出しうるため断定しない語にする |

#### レイアウト

```
┌─ transcript ─────────────────────────────────────────────────┐
│  ▸ read_file  .env                                    1.2 KB │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ DATABASE_URL=••••••••••••••••••••          🔒 masked     ││
│  │ STRIPE_KEY=sk-•••••••••••••••••••          🔒 masked     ││
│  │ LOG_LEVEL=debug                                          ││
│  └──────────────────────────────────────────────────────────┘│
│  🔒 3 values masked in this block          [Reveal block]    │
└───────────────────────────────────────────────────────────────┘
```

#### 状態と操作

| 操作 | 挙動 |
| --- | --- |
| 右レールの `Mask secrets in view`（既定 ON） | セッション全体の一括切替 |
| `Reveal block` | **そのブロックのみ**一時的に解除。セッションを離れると ON に戻る |
| 解除中の表示 | ブロック上部に `🔓 Revealed — masked again when you leave this session` |
| 検出 0 件のとき | マスキング関連の表示を一切出さない（無用な不安を与えない） |

#### 他画面への波及

- **検索結果（`2b`）のスニペット**も露出面。同じ規則を適用し、ヘッダに `🔒 Masking on` を出す。
- **エクスポート**は §4.5 で既定 ON。
- **内蔵ターミナル（§4.3）にはマスキングを適用しない。** ライブの端末出力に介入するとカーソル制御が壊れ、CLI 本来の表示も損なうため。その代わりターミナルタブに `Terminal output is not masked` を常時表示する。

---

### 4.2 Windows 版メイン画面 〔優先: 高〕

**対応要件**: FR-9.1 / NFR-4.1 ／ **理由**: 現状 Windows 表現は Resume ダイアログのみで、
残り 12 画面が macOS 前提。Windows は対等な対象 OS（requirements §2.2）。

#### 作るもの

`1b` と**同一の構造・同一のデータ**で、以下だけを差し替えたアートボード 1 枚。

| 箇所 | macOS 版（既存 `1b`） | Windows 版（新規） |
| --- | --- | --- |
| ウィンドウ装飾 | 左上の信号機ボタン | **右上の** `─ ▢ ✕` |
| タイトルバー | `mochi / codex / Repo identity…` | 同左（配置は左寄せのまま） |
| サイドバーのパス | `~/code/mochi` | `C:\Users\tomy\code\mochi` |
| | `~/work/payments-api` | `C:\work\payments-api` |
| 右レール WORKING DIRECTORY | `~/code/mochi` | `C:\Users\tomy\code\mochi` |
| 右レール ソースファイル | `~/.codex/sessions/2026/09/10/rollout-….jsonl` | `C:\Users\tomy\.codex\sessions\2026\09\10\rollout-….jsonl` |
| 検索ボックス | `Search  ⌘K` | `Search  Ctrl+K` |
| 中央ペイン検索 | `⌘F in session` | `Ctrl+F in session` |
| Resume ボタン | `Resume ⏎` | `Resume  Enter` |

#### あわせて確認すること

- **長いパスの省略**。`C:\Users\...` 形式は `~` 短縮が効かず macOS より明確に長い。サイドバーとレールでの省略方法（先頭省略か中間省略か）を決める。**推奨は中間省略**（`C:\Users\tomy\…\payments-api`）— 末尾のリポジトリ名が最も識別に効くため。
- Windows の長パス（260 文字超、requirements NFR-2.6）に当たったときの表示。

---

### 4.3 内蔵ターミナル 〔優先: 高・新規〕

**対応要件**: FR-7.8 〜 FR-7.8g ／ **理由**: Q-1 で MVP 入り。モック作成時点では対象外だった。

#### 配置

`1b` の**中央ペインにタブを 1 つ追加**する。既存タブと並列にする（別ウィンドウにはしない）。
セッションの「読む」と「動かす」を同じ場所に置くのが `1b` を採用した理由と一致する。

```
Transcript │ Tool calls (38) │ Diffs (11) │ Raw JSONL │ ● Terminal
                                                        ^^^^^^^^^^
                                          実行中は左に稼働インジケータ
```

#### レイアウト

```
┌─ mochi / codex / Repo identity: root commit vs origin URL ──────────────┐
│ Transcript │ Tool calls │ Diffs │ Raw JSONL │ ● Terminal               │
├─────────────────────────────────────────────────────────────────────────┤
│ ● codex · ~/code/mochi          Terminal output is not masked   [⊗ Stop]│
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   codex resume 01J9F7Q3X2ZK4M8C2A          ← 起動コマンドの表示のみ     │
│   ● Resuming session (142 messages)                                     │
│   > _                                       ← 以降は CLI 自身のプロンプト │
│                                                                         │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│ Started 14:31 · running 2m 14s              Scrollback 10,000 lines     │  ← 10,000 は仮置き。M0 の実測で確定（NFR-1.7）
└─────────────────────────────────────────────────────────────────────────┘
```

#### 状態

| 状態 | ヘッダ表示 | 操作 |
| --- | --- | --- |
| 起動中 | `Starting codex…` | — |
| 実行中 | `● codex · <cwd>` ＋経過時間 | `Stop` |
| 正常終了 | `Exited (0) · 4m 02s` | `Run again` / `Close tab` |
| 異常終了 | `Exited (1) · 4m 02s` ＋末尾出力を残す | `Run again` / `Copy output` / `Close tab` |
| 起動失敗 | §4.3 下部の `Resume blocked` カード（既存 `2d`）を中央に表示 | `Set binary path` / `Copy command` |

#### 複数タブ（FR-7.8b）

セッションを跨いで複数の実行を持てる。**サイドバーの実行中セッションに稼働インジケータを出し、
クリックでその実行中ターミナルへ直接移動できること。** タブの識別は `<repo> · <tool>`。

#### 終了時の確認（FR-7.8f）

アプリ終了時に実行中プロセスがあれば必ず確認する。エージェントの作業を無言で打ち切らない。

```
┌─ Sessions are still running ────────────────────────────────┐
│  2 agents are working right now.                            │
│                                                             │
│    ● codex · mochi                     running 2m 14s       │
│    ● claude · payments-api             running 11m 40s      │
│                                                             │
│  Quitting stops them. Their transcripts are already on disk  │
│  and will be there when you come back.                      │
│                                                             │
│                        [Cancel]  [Quit and stop them]        │
└──────────────────────────────────────────────────────────────┘
```

#### 範囲（Q-9 = ① 起動専用で確定）

**対象 CLI の起動専用**であり、汎用シェルとしては提供しない（requirements FR-7.8h）。

| 提供する | 提供しない |
| --- | --- |
| 対象 CLI の起動と、その CLI との対話 | シェルプロンプト（`$` / `>`） |
| リサイズ・スクロール・コピー | シェルの選択（bash / zsh / pwsh…） |
| 停止・再実行 | `cd` などによる作業ディレクトリ変更 |
| — | 環境変数の編集 UI |

プロセス終了後は**プロンプトに戻らず**、タブを終了状態にする（下表参照）。

> **UI 上の表現に関する注意**（requirements FR-7.8i）
> これは隔離でもサンドボックスでもない。起動された CLI 自身は従来どおり任意のコマンドを実行できる
> — それがエージェントの動作そのものである。**「安全」「隔離」「サンドボックス」といった語を
> この画面に使ってはならない。** ここで限定しているのは Mochi の UI の範囲だけで、CLI の権限ではない。

---

### 4.4 削除フロー 〔優先: 高〕

**対応要件**: FR-8.3 / FR-8.3a / FR-8.3b、R-9 ／ **理由**: Q-2 で MVP 入り。**最も危険な操作**。

#### 三段構え

##### 第 1 段: 既定で無効（Settings → Privacy & export）

```
  Allow deleting session files                        [トグル・OFF]
  Off by default. Mochi is a reader; deleting touches the CLIs'
  own files and cannot be undone from here.
```

OFF の間は、一覧・詳細のどこにも削除の導線を出さない。

##### 第 2 段: 確認ダイアログ

**対象の絶対パスと合計サイズを必ず列挙する**（FR-8.3 ②）。件数だけの表示は不可。

```
┌─ Delete 3 session files? ───────────────────────────────────────┐
│                                                                 │
│  These files move to the Trash. Mochi never deletes them        │
│  outright.                                                      │
│                                                                 │
│  ~/.codex/sessions/2026/06/14/rollout-2026-06-14T09-12-40.jsonl │
│                                                        3.4 MB   │
│  ~/.codex/sessions/2026/06/02/rollout-2026-06-02T11-08-02.jsonl │
│                                                        1.1 MB   │
│  ~/.claude/projects/-Users-tomy-code-mochi/01J9F7Q3.jsonl       │
│                                                      812 KB     │
│                                              Total    5.3 MB    │
│                                                                 │
│  AFTER DELETING                                                 │
│  ◉ Keep the conversations in Mochi's index                      │
│    You can still read and search them. This is the point —      │
│    Claude Code deletes its own transcripts after 30 days.       │
│  ○ Remove them from the index too                               │
│    Nothing is left. Not reversible.                             │
│                                                                 │
│                            [Cancel]  [Move 3 files to Trash]    │
└─────────────────────────────────────────────────────────────────┘
```

- 既定は `Keep the conversations in Mochi's index`（FR-8.3a ①）。
- `Cancel` にフォーカスを置く。`Enter` で削除が走らないこと。
- Windows では `Trash` を `Recycle Bin` に、ボタンを `Move 3 files to Recycle Bin` に読み替える。

##### 第 3 段: ゴミ箱へ移せない場合

```
┌─ These files cannot go to the Trash ────────────────────────────┐
│  <理由: 別ボリューム上にある／権限がない など>                    │
│                                                                 │
│  Deleting them now is permanent. There is no undo.              │
│                                                                 │
│                          [Cancel]  [Delete permanently]         │
└─────────────────────────────────────────────────────────────────┘
```

`Delete permanently` は destructive スタイル。ここでも既定フォーカスは `Cancel`。

#### 禁止事項（FR-8.3b）

- **実行中のセッションは選択できない。** 一覧では選択不可にし、理由をツールチップで示す
  （`Running — stop it before deleting`）。
- 削除後に `archived` として残ったセッションは、再度の削除対象にならない（消すファイルがない）。
  インデックスからの除去は §3 の `ARCHIVED` カードの `Remove from index` で行う。

---

### 4.5 エクスポートダイアログ 〔優先: 中〕

**対応要件**: FR-8.4 / FR-8.5 ／ **理由**: `Export` ボタンはあるが挙動が未定。**外部に出る唯一の経路**。

```
┌─ Export session ────────────────────────────────────────────────┐
│  Repo identity: root commit vs origin URL                       │
│  codex · 142 messages · 3.4 MB                                  │
│                                                                 │
│  FORMAT                                                         │
│  ◉ Markdown        Readable. Tool output folded into details.   │
│  ○ JSON            Mochi's normalized shape.                    │
│  ○ Raw JSONL       The original file, copied byte for byte.     │
│                                                                 │
│  INCLUDE                                                        │
│  ☑ Tool calls and results      ☑ Diffs                          │
│  ☐ Thinking blocks             ☐ Subagent transcripts           │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ ☑ Mask secrets                                 🔒           ││
│  │   14 values in this session look like keys or tokens.       ││
│  │   They will be replaced before the file is written.         ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                 │
│                    [Copy to clipboard]  [Cancel]  [Save as…]    │
└─────────────────────────────────────────────────────────────────┘
```

- `Mask secrets` は**既定 ON**（FR-8.5）。
- **OFF にしたときは警告を出す**:
  `⚠ 14 secrets will be written in clear text.`（赤系。ダイアログ内に留め、モーダルは重ねない）
- `Raw JSONL` を選ぶと `Mask secrets` は自動的に OFF になり、無効化される。
  理由を添える: `Raw JSONL is a byte-for-byte copy — masking would change the file.`
- 検出 0 件のときはマスキングのブロックごと非表示。

---

### 4.6 リポジトリの手動マージ／再割り当て 〔優先: 中〕

**対応要件**: FR-3.9、FR-3.3 ／ **理由**: 「同一リポジトリの 2 クローンが別行になる」問題は
モック `1b` のトランスクリプト本文で論じられているのに、ユーザーがそれを直す UI がない。

#### 導線

サイドバーのリポジトリ右クリック → `Merge into…` / `Rename…` / `Hide`。

#### マージダイアログ

**自動判定の根拠を見せた上で、ユーザーに決めさせる**のが要点。

```
┌─ Merge repositories ────────────────────────────────────────────┐
│  payments-api                          637 sessions             │
│  ~/work/payments-api                                            │
│  origin  github.com/acme/payments-api                           │
│  root    4f9c1ab                                                │
│                                                                 │
│  MERGE INTO                                                     │
│  ◉ payments-api  (worktree)            58 sessions              │
│    ~/work/.wt/pa-release                                        │
│    root 4f9c1ab   ✓ same root commit                            │
│                                                                 │
│  ○ payments-api-fork                   12 sessions              │
│    ~/code/payments-api-fork                                     │
│    root 4f9c1ab   ✓ same root commit                            │
│    origin github.com/tomy/payments-api   ⚠ different origin     │
│                                                                 │
│  695 sessions will move under one repository. Tags and notes    │
│  are kept. Session files are not touched.                       │
│                                                                 │
│                                    [Cancel]  [Merge]            │
└─────────────────────────────────────────────────────────────────┘
```

- 一致・不一致の根拠（`same root commit` / `different origin`）を各候補に明示する。
- 逆操作（分割）も必要。**マージは Mochi 側 DB の記録のみで、いつでも取り消せる**旨を書く。

---

### 4.7 ディスク使用量と単価テーブル（Settings に節を追加）〔優先: 中〕

**対応要件**: FR-8.2（UC-03）、FR-10.6 / FR-10.7（Q-7）

#### Settings → Storage（新設）

```
  STORAGE                                                         
  ┌────────────────────────────────────────────────────────────┐  
  │ Session files on disk                            8.7 GB    │  
  │   Codex CLI          ~/.codex/sessions           6.1 GB    │  
  │   Claude Code        ~/.claude/projects          2.4 GB    │  
  │   OpenCode           ~/.local/share/opencode     212 MB    │  
  │                                                            │  
  │ Mochi's own index                                1.4 GB    │  
  └────────────────────────────────────────────────────────────┘  
                                                                  
  BY REPOSITORY                                     [ Show all ]  
    payments-api                                      3.9 GB      
    mochi                                             2.1 GB      
    dotfiles                                          890 MB      
                                                                  
  OpenCode never removes old sessions on its own.                 
```

各行から絞り込み済みのセッション一覧へ移動できること（UC-03 の棚卸しにつなげる）。

#### Settings → Cost（新設、FR-10.6）

```
  TOKEN PRICING                                                   
  Prices are yours to maintain. Mochi ships defaults and never    
  overwrites your edits on update.                                
                                                                  
  pricing.toml · %APPDATA%\Mochi              [ Open ] [ Reset ]  
  Rates as of  2026-08-01                                         
                                                                  
  gpt-5-codex          in $1.25 / 1M    out $10.00 / 1M           
  claude-opus-5        in $5.00 / 1M    out $25.00 / 1M           
  claude-sonnet-5      in $3.00 / 1M    out $15.00 / 1M           
  sonnet-4.5           — not in table —          ⚠ Unknown model  
                                                                  
  Estimates only. They will not match your actual bill.           
```

- **テーブルにないモデルはコストを表示しない**（FR-10.6 ④）。0 円として合計してはならない。
  該当セッションのコスト欄は `—` とし、ツールチップで `No price for sonnet-4.5` と示す。
- 上記の単価は**書式を示すための記入例**であり、正しい値ではない。実装時に確認すること。

---

### 4.7b Settings → About（Apache-2.0 対応）〔優先: 中〕

**対応要件**: NFR-4b.1〜1c、NFR-4b.3（Q-8 = Apache License 2.0）

既存モック `2a` の左メニューに `About` はあるが、中身が未定。OSS 化に伴い**必須**の内容がある。

```
  ABOUT
  Mochi 0.1.0-dev
  A session manager for Codex CLI, Claude Code and OpenCode.

  LICENSE
  Apache License 2.0                                    [ View ]
  Copyright 2026 <著作権者>

  This product includes software developed by third parties.
  See NOTICE for attributions.                          [ View ]

  THIRD-PARTY LICENSES                                  [ View all ]
    eframe / egui      MIT OR Apache-2.0
    rusqlite           MIT
    wgpu               MIT OR Apache-2.0
    …                                                   250 packages

  Source code                     github.com/<owner>/mochi
  Report a security issue         SECURITY.md
```

- `NOTICE` の内容をアプリ内から参照できること（NFR-4b.1a）。再配布者が引き継ぐ義務があるため、
  「どこかにある」ではなく明示的な導線を置く。
- 第三者ライセンス一覧はビルド時に依存関係から自動生成する。手書きしない（更新漏れが必ず起きる）。

---

### 4.8 サブエージェントのトランスクリプト 〔優先: 低〕

**対応要件**: FR-5.7 ／ Claude Code の `<session>/subagents/` に対応。

トランスクリプト内でサブエージェントが起動された箇所に、展開可能な行を差し込む。

```
   ▸ Task  Explore the adapter trait               3 agents · 12.4k tokens
     ├ agent-01  search the repo for ToolAdapter        →  open
     ├ agent-02  read the OpenCode adapter               →  open
     └ agent-03  summarize differences                   →  open
```

`open` で中央ペインにサブエージェントのトランスクリプトを開き、**ブレッドクラムに親を残す**
（`mochi / claude / Wire the incremental rescan / agent-02`）。親へ戻れること。

---

### 4.9 ライトテーマ 〔優先: 低〕

**対応要件**: FR-9.2 ／ 現状は `1a` に切替アイコンがあるだけ。

`1b` のライト版を 1 枚。既存のダーク配色と**同じ意味の割り当て**を保つこと。

| 意味 | ダーク | ライトで確認すべきこと |
| --- | --- | --- |
| アクセント／主要操作 | `#F0968C` | 白背景でコントラスト比 4.5:1 を満たすか（NFR-6.2）。**満たさないため濃くする必要がある** |
| 追加行（diff） | 緑背景 | 同上 |
| 削除行（diff） | 赤背景 | 同上 |
| 警告・危険 | 赤系 | アクセント色と区別がつくか |
| 実行中インジケータ | 緑 | 同上 |

**注意**: 現在のアクセント `#F0968C` は暗い背景を前提にした明度で、白背景では十分なコントラストが出ない。
ライトテーマではトークンの値を差し替える前提で設計すること（同じ色の使い回しはしない）。

---

## 5. 状態遷移

### 5.1 セッションの状態

```
                    ┌──────────────────────────────┐
   スキャンで発見 ──▶│           ok / partial        │
                    └──────────────────────────────┘
                        │              │          │
     元ファイル削除     │              │          │  cwd が消えた
     （CLI の保持期間  │              │          ▼
       or FR-8.3 ①）  │              │   ┌──────────────────┐
                       ▼              │   │   cwd_missing     │
              ┌──────────────┐        │   └──────────────────┘
              │   archived    │        │          │ Relocate（FR-7.6a）
              │  閲覧のみ     │        │          ▼
              └──────────────┘        │      ok に戻る
                       │              │
        Remove from    │              │ パース不能
        index          ▼              ▼
                  （消滅）      ┌──────────────┐
                                │ read_failed   │
                                └──────────────┘
```

`running` は上記と直交する一時状態で、内蔵ターミナルのプロセス終了、
またはファイル監視の無更新タイムアウトで解除される。

### 5.2 再開の分岐（FR-7.1 / FR-7.6 / FR-1.6）

```
  [Resume] 押下
      │
      ├─ 状態が archived ────────────▶ ボタン自体を無効化（理由をツールチップ）
      ├─ cwd が存在しない ───────────▶ Working directory missing カード
      ├─ CLI 実行ファイルが見つからない ▶ Resume blocked カード（既存 2d）
      │
      └─ すべて満たす
             ├─ 実行先 = 内蔵（既定） ─▶ Terminal タブを開いて実行（§4.3）
             └─ 実行先 = 外部 ─────────▶ Resume dialog（既存 1c）→ 外部ターミナル起動
```

---

## 6. 作成チェックリスト

### 第 1 ラウンド — ✅ 完了（2026-09-10）

- [x] M-1〜M-5 モック修正
- [x] §4.1〜§4.9 新規 10 画面（19 アートボード）
- [x] M-3 の処理モデル → **逐次処理**で確定

### 第 2 ラウンド — 軽微、実装着手前に反映

- [ ] M-6 `3b` の `$` を `▸` に
- [ ] M-7 `3a` の diff ヘッダを `src/repo/identity.rs` に戻す
- [ ] M-8 `3a` / `3i` の主ボタンを `Open in editor` に統一し、reveal 操作をパスの右クリックへ
- [ ] M-9 `1b` に `● Terminal` タブと `Est. cost` 行（ツールチップ付き）を追加
- [ ] M-10 `ARCHIVED` カードの補足行を事実＋一般的原因の形に
- [ ] M-11 `1b` / `3a` / `3i` の左サイドバーを 2 ペイン（リポジトリ／セッション）に差し替え
