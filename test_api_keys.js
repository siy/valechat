// Test script to verify API key persistence
const { invoke } = require('@tauri-apps/api/core');

async function testApiKeyPersistence() {
    const testApiKey = 'sk-test123456789';
    const provider = 'openai';
    
    try {
        console.log('Testing API key storage and retrieval...');
        
        // Test storing an API key
        console.log('1. Storing API key for OpenAI...');
        await invoke('set_api_key', {
            provider: provider,
            api_key: testApiKey
        });
        console.log('✓ API key stored successfully');
        
        // Test retrieving the API key
        console.log('2. Retrieving API key for OpenAI...');
        const retrievedKey = await invoke('get_api_key', {
            provider: provider
        });
        
        if (retrievedKey === testApiKey) {
            console.log('✓ API key retrieved successfully and matches stored value');
        } else {
            console.log('✗ API key retrieval failed or doesn\'t match');
            console.log('Expected:', testApiKey);
            console.log('Got:', retrievedKey);
        }
        
        // Test removing the API key
        console.log('3. Removing API key for OpenAI...');
        await invoke('remove_api_key', {
            provider: provider
        });
        console.log('✓ API key removed successfully');
        
        // Verify removal
        console.log('4. Verifying removal...');
        const removedKey = await invoke('get_api_key', {
            provider: provider
        });
        
        if (removedKey === null || removedKey === undefined) {
            console.log('✓ API key successfully removed');
        } else {
            console.log('✗ API key removal failed');
            console.log('Expected: null/undefined');
            console.log('Got:', removedKey);
        }
        
        console.log('\nAPI key persistence tests completed!');
        
    } catch (error) {
        console.error('Test failed:', error);
    }
}

// Export for use in the app
if (typeof window !== 'undefined') {
    window.testApiKeyPersistence = testApiKeyPersistence;
}

module.exports = { testApiKeyPersistence };