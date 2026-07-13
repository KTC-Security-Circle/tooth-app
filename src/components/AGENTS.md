# AGENTS.md (src/components)

フロントエンドのコンポーネント設計方針とディレクトリ構成。

## Directory Structure

```
src/components/
├─ ui/                    # 横断して使われる汎用コンポーネント
└─ app/                   # 特定の機能やページで使い回せるコンポーネント
```

### `ui/` — 汎用コンポーネント

アプリケーション全体で横断的に使われる、ビジネスロジックを含まない純粋な UI コンポーネント。特定の機能やページに依存しない。

### `app/` — 機能特化コンポーネント

特定の機能やページで使い回せるコンポーネント。`ui/` の汎用コンポーネントを組み合わせて構成され、アプリケーションのドメイン知識を含む。

## Component Conventions

### フォルダ構成

1コンポーネント = 1フォルダ。エントリは `index.tsx`。

```
Component/
└─ index.tsx
```

サブコンポーネント (Compound Component パターン) がある場合は、サブコンポーネントも同様にフォルダ + `index.tsx` で定義し、親の `index.tsx` で import して静的プロパティとして付与する。

```
Component/
├─ index.tsx              # 本体 + SubComponent を静的プロパティで付与
├─ SubComponent/
│  ├─ index.tsx
```

### コンポーネントの形式

- `interface Props` で props を定義 (HTML 要素の props を引き継ぐものは `extends React.ComponentPropsWithoutRef<'...'>`)
- `React.FC<Props>` + アロー関数で定義
- デフォルトエクスポート (`export default`)

```tsx
interface Props {
  ...
}

const Component: React.FC<Props> = ({ ... }) => {
  // impl
}

export default Component
```

### Compound Component パターン

特定のコンポーネントと組み合わせて用いるようなコンポーネントは親コンポーネントの静的プロパティとして公開する。親の `index.tsx` で `Component.SubComponent = SubComponent` のように付与する。

```tsx
const Component: React.FC<Props> & {
  SubComponent: typeof SubComponent
} = ({ ... }) => { ... }

Component.SubComponent = SubComponent

export default Component
```

使用側:

```tsx
<Component>
  <Component.SubComponent>...</Component.SubComponent>
</Component>
```

### バレルエクスポートしない

`index.ts` での一括再エクスポートは行わない。使用側は各コンポーネントフォルダから直接 import する。

```ts
import Button from "@/components/ui/Button";
import FormField from "@/components/ui/FormField";
```

## Code Style

- Tailwind クラスの結合には `@/lib/cn` の `cn()` を使用する
