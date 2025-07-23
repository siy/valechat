import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import './App.css'

interface AppInfo {
  name: string
  version: string
  description: string
}

function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const [models, setModels] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const initializeApp = async () => {
      try {
        // Get app info
        const info = await invoke<AppInfo>('get_app_info')
        setAppInfo(info)

        // Get available models
        const availableModels = await invoke<string[]>('get_models')
        setModels(availableModels)
      } catch (err) {
        console.error('Failed to initialize app:', err)
        setError(err instanceof Error ? err.message : 'Unknown error')
      } finally {
        setLoading(false)
      }
    }

    initializeApp()
  }, [])

  const handleSendMessage = async () => {
    try {
      const response = await invoke<string>('send_message', {
        message: 'Hello from the frontend!',
        model: 'gpt-4'
      })
      console.log('Response:', response)
    } catch (err) {
      console.error('Failed to send message:', err)
    }
  }

  if (loading) {
    return (
      <div className="loading">
        <h2>Loading ValeChat...</h2>
      </div>
    )
  }

  if (error) {
    return (
      <div className="error">
        <h2>Error</h2>
        <p>{error}</p>
      </div>
    )
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>{appInfo?.name || 'ValeChat'}</h1>
        <p>Version {appInfo?.version}</p>
        <p>{appInfo?.description}</p>
      </header>

      <main className="app-main">
        <section className="models-section">
          <h2>Available Models</h2>
          {models.length > 0 ? (
            <ul>
              {models.map((model, index) => (
                <li key={index}>{model}</li>
              ))}
            </ul>
          ) : (
            <p>No models configured yet. Please add API keys in settings.</p>
          )}
        </section>

        <section className="chat-section">
          <h2>Chat Interface</h2>
          <div className="chat-placeholder">
            <p>Chat interface will be implemented in Phase 2</p>
            <button onClick={handleSendMessage}>
              Test Message Send
            </button>
          </div>
        </section>
      </main>

      <footer className="app-footer">
        <p>ValeChat - Multi-model AI Chat Application</p>
      </footer>
    </div>
  )
}

export default App