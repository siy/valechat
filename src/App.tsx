import ChatWindow from './components/chat/ChatWindow'
import './App.css'

// Debug function for testing
(window as any).testCreateConversation = async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  
  try {
    console.log('Testing test_create_conversation...');
    const result1 = await invoke('test_create_conversation');
    console.log('test_create_conversation result:', result1);
  } catch (e) {
    console.error('test_create_conversation error:', e);
  }
  
  try {
    console.log('Testing create_conversation_simple...');
    const result2 = await invoke('create_conversation_simple', { title: 'Test Title' });
    console.log('create_conversation_simple result:', result2);
  } catch (e) {
    console.error('create_conversation_simple error:', e);
  }
  
  try {
    console.log('Testing create_conversation...');
    const result3 = await invoke('create_conversation', { request: { title: 'Test Title' } });
    console.log('create_conversation result:', result3);
  } catch (e) {
    console.error('create_conversation error:', e);
  }
};

// Debug function for testing message sending
(window as any).testSendMessage = async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  
  try {
    console.log('Testing send_message...');
    const result = await invoke('send_message', {
      request: {
        conversation_id: 'test-123', // Use the test conversation we created
        content: 'Hello, test message',
        model: 'gpt-3.5-turbo',
        provider: 'openai'
      }
    });
    console.log('send_message result:', result);
  } catch (e) {
    console.error('send_message error:', e);
  }
};

function App() {
  return (
    <div className="app">
      <ChatWindow />
    </div>
  )
}

export default App