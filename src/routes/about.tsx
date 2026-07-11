import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/about')({
  component: () => (
    <div className="p-2">
      <h3>About</h3>
      <p>This is a Tauri app with TanStack Router.</p>
    </div>
  ),
})
