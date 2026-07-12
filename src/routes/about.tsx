import { createFileRoute } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'

function About() {
  const [cameras, setCameras] = useState<Array<{
    name: string
    path: string
  }> | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleListCameras = async () => {
    setLoading(true)
    setError(null)
    try {
      const result =
        await invoke<Array<{ name: string; path: string }>>('list_cameras')
      setCameras(result)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="p-2">
      <h3>About</h3>
      <p>This is a Tauri app with TanStack Router.</p>

      <hr className="my-4" />
      <h4 className="mb-2 font-semibold text-lg">カメラ一覧</h4>

      <button
        type="button"
        disabled={loading}
        onClick={handleListCameras}
        className="cursor-pointer rounded-lg border border-transparent bg-white px-[1.2em] py-[0.6em] font-medium text-[#0f0f0f] text-base shadow-[0_2px_2px_rgba(0,0,0,0.2)] outline-none transition-colors duration-250 hover:border-[#396cd8] active:border-[#396cd8] active:bg-[#e8e8e8] disabled:cursor-not-allowed disabled:opacity-50 dark:bg-[#0f0f0f98] dark:text-white dark:active:bg-[#0f0f0f69]"
      >
        {loading ? '取得中...' : 'カメラ一覧を取得'}
      </button>

      {error !== null && <p className="mt-2 text-red-500">{error}</p>}

      {cameras !== null && cameras.length === 0 && (
        <p className="mt-2">カメラが見つかりませんでした</p>
      )}

      {cameras !== null && cameras.length > 0 && (
        <ol className="mt-2 list-inside list-decimal">
          {cameras.map((camera) => (
            <li key={camera.path}>
              {camera.name} ({camera.path})
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}

export const Route = createFileRoute('/about')({
  component: About,
})
