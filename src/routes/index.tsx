import { createFileRoute } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { useState } from 'react'
import reactLogo from '../assets/react.svg'

export const Route = createFileRoute('/')({
  component: function Index() {
    const [greetMsg, setGreetMsg] = useState('')
    const [name, setName] = useState('')

    async function greet() {
      setGreetMsg(await invoke('greet', { name }))
    }

    return (
      <main className="m-0 flex flex-col justify-center pt-[10vh] text-center">
        <h1>Welcome to Tauri + React</h1>

        <div className="flex justify-center">
          <a
            href="https://vite.dev"
            target="_blank"
            rel="noopener"
            className="font-medium text-[#646cff] no-underline hover:text-[#535bf2] dark:hover:text-[#24c8db]"
          >
            <img
              src="/vite.svg"
              className="h-24 p-6 transition-all duration-750 will-change-[filter] hover:drop-shadow-[0_0_2em_#747bff]"
              alt="Vite logo"
            />
          </a>
          <a
            href="https://tauri.app"
            target="_blank"
            rel="noopener"
            className="font-medium text-[#646cff] no-underline hover:text-[#535bf2] dark:hover:text-[#24c8db]"
          >
            <img
              src="/tauri.svg"
              className="h-24 p-6 transition-all duration-750 will-change-[filter] hover:drop-shadow-[0_0_2em_#24c8db]"
              alt="Tauri logo"
            />
          </a>
          <a
            href="https://react.dev"
            target="_blank"
            rel="noopener"
            className="font-medium text-[#646cff] no-underline hover:text-[#535bf2] dark:hover:text-[#24c8db]"
          >
            <img
              src={reactLogo}
              className="h-24 p-6 transition-all duration-750 will-change-[filter] hover:drop-shadow-[0_0_2em_#61dafb]"
              alt="React logo"
            />
          </a>
        </div>
        <p>Click on the Tauri, Vite, and React logos to learn more.</p>

        <form
          className="flex justify-center"
          onSubmit={(e) => {
            e.preventDefault()
            greet()
          }}
        >
          <input
            id="greet-input"
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder="Enter a name..."
            className="mr-1.25 rounded-lg border border-transparent bg-white px-[1.2em] py-[0.6em] font-medium text-[#0f0f0f] text-base shadow-[0_2px_2px_rgba(0,0,0,0.2)] outline-none transition-colors duration-250 dark:bg-[#0f0f0f98] dark:text-white"
          />
          <button
            type="submit"
            className="cursor-pointer rounded-lg border border-transparent bg-white px-[1.2em] py-[0.6em] font-medium text-[#0f0f0f] text-base shadow-[0_2px_2px_rgba(0,0,0,0.2)] outline-none transition-colors duration-250 hover:border-[#396cd8] active:border-[#396cd8] active:bg-[#e8e8e8] dark:bg-[#0f0f0f98] dark:text-white dark:active:bg-[#0f0f0f69]"
          >
            Greet
          </button>
        </form>
        <p>{greetMsg}</p>
      </main>
    )
  },
})
