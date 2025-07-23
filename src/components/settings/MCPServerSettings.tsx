import React, { useState } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { MCPServer } from '../../types';

interface MCPServerSettingsProps {
  onSettingsChange: () => void;
}

const MCPServerSettings: React.FC<MCPServerSettingsProps> = ({ onSettingsChange }) => {
  const { config, addMCPServer, updateMCPServer, removeMCPServer } = useSettingsStore();
  const [showAddForm, setShowAddForm] = useState(false);
  const [expandedServer, setExpandedServer] = useState<string | null>(null);
  const [newServer, setNewServer] = useState({
    name: '',
    command: '',
    args: '',
  });

  const handleAddServer = () => {
    if (!newServer.name || !newServer.command) return;

    const server: MCPServer = {
      id: Date.now().toString(),
      name: newServer.name,
      command: newServer.command,
      args: newServer.args ? newServer.args.split(' ').filter(arg => arg.trim()) : [],
      enabled: true,
      status: 'stopped',
      tools: [],
      resources: [],
    };

    addMCPServer(server);
    setNewServer({ name: '', command: '', args: '' });
    setShowAddForm(false);
    onSettingsChange();
  };

  const handleServerToggle = (serverId: string, enabled: boolean) => {
    updateMCPServer(serverId, { enabled });
    onSettingsChange();
  };

  const handleRemoveServer = (serverId: string) => {
    if (window.confirm('Are you sure you want to remove this MCP server?')) {
      removeMCPServer(serverId);
      onSettingsChange();
    }
  };

  const toggleServerExpansion = (serverId: string) => {
    setExpandedServer(expandedServer === serverId ? null : serverId);
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'running': return '#16a34a';
      case 'error': return '#dc2626';
      case 'starting': return '#ea580c';
      default: return '#6b7280';
    }
  };

  const getStatusText = (status: string) => {
    switch (status) {
      case 'running': return 'Running';
      case 'error': return 'Error';
      case 'starting': return 'Starting';
      default: return 'Stopped';
    }
  };

  return (
    <div className="settings-section">
      <h3 className="settings-section-title">MCP Servers</h3>
      <p style={{ color: 'var(--text-secondary)', marginBottom: '24px', fontSize: '14px' }}>
        Model Context Protocol (MCP) servers provide tools and resources that AI models can use to extend their capabilities.
      </p>

      <div className="settings-group">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
          <h4 style={{ margin: 0, color: 'var(--text-primary)' }}>Configured Servers</h4>
          <button
            className="btn btn-primary"
            onClick={() => setShowAddForm(!showAddForm)}
          >
            {showAddForm ? 'Cancel' : '+ Add Server'}
          </button>
        </div>

        {showAddForm && (
          <div style={{
            padding: '16px',
            background: 'var(--bg-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: '8px',
            marginBottom: '16px'
          }}>
            <h4 style={{ margin: '0 0 12px 0', color: 'var(--text-primary)' }}>Add New MCP Server</h4>
            
            <div style={{ display: 'grid', gap: '12px' }}>
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontSize: '14px', fontWeight: 500 }}>
                  Server Name
                </label>
                <input
                  type="text"
                  className="form-input"
                  placeholder="e.g., File System Tools"
                  value={newServer.name}
                  onChange={(e) => setNewServer(prev => ({ ...prev, name: e.target.value }))}
                />
              </div>
              
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontSize: '14px', fontWeight: 500 }}>
                  Command
                </label>
                <input
                  type="text"
                  className="form-input"
                  placeholder="e.g., /usr/local/bin/mcp-server-filesystem"
                  value={newServer.command}
                  onChange={(e) => setNewServer(prev => ({ ...prev, command: e.target.value }))}
                />
              </div>
              
              <div>
                <label style={{ display: 'block', marginBottom: '4px', fontSize: '14px', fontWeight: 500 }}>
                  Arguments (optional)
                </label>
                <input
                  type="text"
                  className="form-input"
                  placeholder="e.g., --root /home/user/projects"
                  value={newServer.args}
                  onChange={(e) => setNewServer(prev => ({ ...prev, args: e.target.value }))}
                />
              </div>
              
              <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                <button className="btn" onClick={() => setShowAddForm(false)}>
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleAddServer}
                  disabled={!newServer.name || !newServer.command}
                >
                  Add Server
                </button>
              </div>
            </div>
          </div>
        )}

        {config.mcp_servers.length === 0 ? (
          <div style={{
            padding: '32px',
            textAlign: 'center',
            color: 'var(--text-secondary)',
            border: '1px dashed var(--border-color)',
            borderRadius: '8px'
          }}>
            <p style={{ margin: '0 0 8px 0' }}>No MCP servers configured</p>
            <p style={{ margin: 0, fontSize: '14px' }}>Add a server to extend AI model capabilities with tools and resources.</p>
          </div>
        ) : (
          config.mcp_servers.map((server) => (
            <div key={server.id} style={{
              border: '1px solid var(--border-color)',
              borderRadius: '8px',
              marginBottom: '12px',
              overflow: 'hidden'
            }}>
              <div style={{ padding: '16px' }}>
                <div className="settings-row">
                  <div className="settings-label">
                    <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                      <h4>{server.name}</h4>
                      <span
                        className="status-indicator"
                        style={{
                          background: server.enabled ? '#dcfce7' : '#f3f4f6',
                          color: server.enabled ? getStatusColor(server.status) : 'var(--text-secondary)',
                        }}
                      >
                        ● {server.enabled ? getStatusText(server.status) : 'Disabled'}
                      </span>
                    </div>
                    <p style={{ fontSize: '13px', fontFamily: 'monospace', color: 'var(--text-secondary)' }}>
                      {server.command} {server.args.join(' ')}
                    </p>
                  </div>
                  <div className="settings-control" style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    <label className="form-switch">
                      <input
                        type="checkbox"
                        checked={server.enabled}
                        onChange={(e) => handleServerToggle(server.id, e.target.checked)}
                      />
                      <span className="form-switch-slider"></span>
                    </label>
                    <button
                      className="btn btn-small"
                      onClick={() => toggleServerExpansion(server.id)}
                    >
                      {expandedServer === server.id ? '▼' : '▶'}
                    </button>
                    <button
                      className="btn btn-small btn-danger"
                      onClick={() => handleRemoveServer(server.id)}
                    >
                      🗑
                    </button>
                  </div>
                </div>
              </div>

              {expandedServer === server.id && (
                <div style={{
                  padding: '16px',
                  background: 'var(--bg-primary)',
                  borderTop: '1px solid var(--border-color)'
                }}>
                  {server.tools.length > 0 && (
                    <div style={{ marginBottom: '16px' }}>
                      <h4 style={{ margin: '0 0 8px 0', color: 'var(--text-primary)' }}>
                        Available Tools ({server.tools.length})
                      </h4>
                      <div style={{ display: 'grid', gap: '4px' }}>
                        {server.tools.map((tool, index) => (
                          <div key={index} style={{
                            padding: '8px 12px',
                            background: 'var(--bg-secondary)',
                            borderRadius: '4px',
                            fontSize: '14px'
                          }}>
                            <div style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                              {tool.name}
                            </div>
                            <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>
                              {tool.description}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {server.resources.length > 0 && (
                    <div>
                      <h4 style={{ margin: '0 0 8px 0', color: 'var(--text-primary)' }}>
                        Available Resources ({server.resources.length})
                      </h4>
                      <div style={{ display: 'grid', gap: '4px' }}>
                        {server.resources.map((resource, index) => (
                          <div key={index} style={{
                            padding: '8px 12px',
                            background: 'var(--bg-secondary)',
                            borderRadius: '4px',
                            fontSize: '14px'
                          }}>
                            <div style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                              {resource.name}
                            </div>
                            <div style={{ color: 'var(--text-secondary)', fontSize: '12px', fontFamily: 'monospace' }}>
                              {resource.uri}
                            </div>
                            {resource.description && (
                              <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>
                                {resource.description}
                              </div>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {server.tools.length === 0 && server.resources.length === 0 && (
                    <div style={{ color: 'var(--text-secondary)', fontSize: '14px', textAlign: 'center', padding: '16px' }}>
                      {server.status === 'running' ? 'No tools or resources available' : 'Start server to see available tools and resources'}
                    </div>
                  )}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
};

export default MCPServerSettings;