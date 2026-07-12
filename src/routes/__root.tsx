import { createRootRoute, Link, Outlet } from '@tanstack/react-router'
import { TanStackRouterDevtools } from '@tanstack/react-router-devtools'

export const Route = createRootRoute({
  component: () => (
    <div className="flex min-h-screen flex-col bg-[#f6f6f6] text-[#0f0f0f]">
      <nav className="border-slate-800 border-b bg-slate-900 px-4 py-3">
        <div className="mx-auto flex max-w-4xl items-center gap-6">
          <span className="font-semibold text-sm text-white">
            Tooth Calibrator
          </span>
          <div className="flex gap-4">
            <Link
              to="/"
              activeProps={{ className: 'text-white' }}
              className="font-medium text-slate-300 text-sm transition-colors hover:text-white"
            >
              Home
            </Link>
            <Link
              to="/settings"
              activeProps={{ className: 'text-white' }}
              className="font-medium text-slate-300 text-sm transition-colors hover:text-white"
            >
              設定
            </Link>
          </div>
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
      <p className="text-slate-600 text-sm">Page not found</p>
      <Link
        to="/"
        className="mt-2 inline-block font-medium text-slate-800 text-sm hover:underline"
      >
        Go Home
      </Link>
    </div>
  ),
})
