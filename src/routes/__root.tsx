import { createRootRoute, Link, Outlet } from '@tanstack/react-router'
import { TanStackRouterDevtools } from '@tanstack/react-router-devtools'

export const Route = createRootRoute({
  component: () => (
    <div className="flex min-h-screen flex-col bg-background text-foreground">
      <nav className="border-border border-b bg-card px-4 py-3">
        <div className="mx-auto flex max-w-4xl items-center justify-between">
          <span className="font-semibold text-foreground text-sm">
            Tooth Calibrator
          </span>
          <Link
            to="/settings"
            aria-label="設定"
            className="rounded-md p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background"
          >
            <span className="icon-gear" />
          </Link>
        </div>
      </nav>
      <main className="flex-1">
        <Outlet />
      </main>
      <TanStackRouterDevtools />
    </div>
  ),
  notFoundComponent: () => (
    <div className="p-6">
      <p className="text-muted-foreground text-sm">Page not found</p>
      <Link
        to="/"
        className="mt-2 inline-block font-medium text-foreground text-sm hover:underline"
      >
        Go Home
      </Link>
    </div>
  ),
})
