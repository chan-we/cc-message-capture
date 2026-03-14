import { useState, useEffect, useRef } from 'react'
import {
  Button,
  InputNumber,
  Space,
  Table,
  Tag,
  Tabs,
  message,
  Empty,
  Badge,
  Splitter,
  Dropdown,
} from 'antd'
import {
  PlayCircleOutlined,
  PauseCircleOutlined,
  DeleteOutlined,
  SafetyCertificateOutlined,
  CopyOutlined,
  RobotOutlined,
} from '@ant-design/icons'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { save } from '@tauri-apps/plugin-dialog'
import type { CapturedMessage, ProxyStatus, CertStatus } from '@/types'
import { isClaudeApiRequest } from '@/services/claudeParser'
import ClaudeMessageViewer from '@/components/ClaudeMessageViewer'

const methodColors: Record<string, string> = {
  GET: 'blue',
  POST: 'green',
  PUT: 'orange',
  DELETE: 'red',
  PATCH: 'purple',
  OPTIONS: 'default',
  HEAD: 'default',
  CONNECT: 'cyan',
}

const statusColor = (status: number) => {
  if (status >= 200 && status < 300) return 'green'
  if (status >= 300 && status < 400) return 'blue'
  if (status >= 400 && status < 500) return 'orange'
  return 'red'
}

function formatJson(str: string): string {
  try {
    return JSON.stringify(JSON.parse(str), null, 2)
  } catch {
    return str
  }
}

function HeadersTable({ headers }: { headers: Record<string, string> }) {
  const data = Object.entries(headers).map(([key, value]) => ({
    key,
    name: key,
    value,
  }))
  return (
    <Table
      dataSource={data}
      columns={[
        { title: '请求头', dataIndex: 'name', width: 200, ellipsis: true },
        { title: '值', dataIndex: 'value', ellipsis: true },
      ]}
      size="small"
      pagination={false}
      style={{ marginTop: 8 }}
    />
  )
}

function BodyViewer({ body, label }: { body: string; label: string }) {
  if (!body) return <Empty description={`无${label}内容`} />
  const formatted = formatJson(body)
  return (
    <div style={{ position: 'relative' }}>
      <Button
        icon={<CopyOutlined />}
        size="small"
        style={{ position: 'absolute', right: 8, top: 8, zIndex: 1 }}
        onClick={() => {
          navigator.clipboard.writeText(formatted)
          message.success('已复制')
        }}
      />
      <pre
        style={{
          background: '#f5f5f5',
          padding: '12px',
          borderRadius: 6,
          overflow: 'auto',
          maxHeight: 500,
          fontSize: 12,
          lineHeight: 1.5,
        }}
      >
        {formatted}
      </pre>
    </div>
  )
}

export default function Capture() {
  const [messages, setMessages] = useState<CapturedMessage[]>([])
  const [selected, setSelected] = useState<CapturedMessage | null>(null)
  const [running, setRunning] = useState(false)
  const [port, setPort] = useState(9898)
  const [certStatus, setCertStatus] = useState<CertStatus | null>(null)
  const listenerRef = useRef<(() => void) | null>(null)

  useEffect(() => {
    invoke<ProxyStatus>('get_proxy_status').then((status) => {
      setRunning(status.running)
      setPort(status.port)
    })

    // Check certificate status on mount
    invoke<CertStatus>('check_cert_status').then((status) => {
      setCertStatus(status)
    })

    const setup = async () => {
      const unlisten = await listen<CapturedMessage>('captured-message', (event) => {
        setMessages((prev) => [event.payload, ...prev])
      })
      listenerRef.current = unlisten
    }
    setup()

    return () => {
      listenerRef.current?.()
    }
  }, [])

  const handleStart = async () => {
    try {
      await invoke('start_proxy', { port })
      setRunning(true)
      message.success(`代理已启动，端口 ${port}`)
    } catch (e) {
      message.error(`失败: ${e}`)
    }
  }

  const handleStop = async () => {
    try {
      await invoke('stop_proxy')
      setRunning(false)
      message.success('代理已停止')
    } catch (e) {
      message.error(`失败: ${e}`)
    }
  }

  const handleExportCert = async () => {
    try {
      // Open save dialog to let user choose export location
      const destPath = await save({
        defaultPath: 'mitmproxy-ca-cert.pem',
        filters: [{ name: 'Certificate', extensions: ['pem', 'crt', 'cer'] }],
      })

      if (!destPath) {
        return // User cancelled
      }

      await invoke('export_ca_cert', { destPath })
      message.success(`证书已导出到: ${destPath}`)
    } catch (e) {
      message.error(`导出失败: ${e}`)
    }
  }

  const handleInstallCert = async () => {
    try {
      const result = await invoke<string>('install_ca_cert')
      message.success(result)
      // Refresh status after install
      const status = await invoke<CertStatus>('check_cert_status')
      setCertStatus(status)
    } catch (e) {
      message.error(`安装失败: ${e}`)
    }
  }

  const handleCheckCertStatus = async () => {
    try {
      const status = await invoke<CertStatus>('check_cert_status')
      if (status.installed) {
        message.success(status.details)
      } else {
        message.warning(status.details)
      }
      return status
    } catch (e) {
      message.error(`检查失败: ${e}`)
      return null
    }
  }

  const handleUninstallCert = async () => {
    try {
      const result = await invoke<string>('uninstall_ca_cert')
      message.success(result)
      // Refresh status after uninstall
      const status = await invoke<CertStatus>('check_cert_status')
      setCertStatus(status)
    } catch (e) {
      message.error(`卸载失败: ${e}`)
    }
  }

  const columns = [
    {
      title: '方法',
      dataIndex: 'method',
      width: 80,
      render: (m: string) => <Tag color={methodColors[m] || 'default'}>{m}</Tag>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 70,
      render: (s: number) => <Tag color={statusColor(s)}>{s}</Tag>,
    },
    {
      title: 'URL',
      dataIndex: 'url',
      ellipsis: true,
    },
    {
      title: '耗时',
      dataIndex: 'duration_ms',
      width: 90,
      render: (d: number) => `${d}ms`,
    },
    {
      title: '时间',
      dataIndex: 'timestamp',
      width: 90,
      render: (t: string) => new Date(t).toLocaleTimeString(),
    },
  ]

  return (
    <div style={{ height: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Toolbar */}
      <div
        style={{
          padding: '12px 16px',
          borderBottom: '1px solid #f0f0f0',
          background: '#fafafa',
        }}
      >
        <Space wrap>
          {running ? (
            <Button
              type="primary"
              danger
              icon={<PauseCircleOutlined />}
              onClick={handleStop}
            >
              停止
            </Button>
          ) : (
            <Button
              type="primary"
              icon={<PlayCircleOutlined />}
              onClick={handleStart}
            >
              启动
            </Button>
          )}
          <Badge status={running ? 'success' : 'default'} text={running ? '运行中' : '已停止'} />
          <InputNumber
            addonBefore="端口"
            value={port}
            onChange={(v) => v && setPort(v)}
            disabled={running}
            min={1024}
            max={65535}
            style={{ width: 160 }}
          />
          <Dropdown
            menu={{
              items: [
                {
                  key: 'status',
                  label: certStatus?.installed ? '已安装' : '未安装',
                  onClick: handleCheckCertStatus,
                },
                { type: 'divider' },
                {
                  key: 'export',
                  label: '导出证书',
                  onClick: handleExportCert,
                },
                ...(certStatus?.installed
                  ? [
                      {
                        key: 'uninstall',
                        label: '卸载证书',
                        onClick: handleUninstallCert,
                      },
                    ]
                  : [
                      {
                        key: 'install',
                        label: '安装证书',
                        onClick: handleInstallCert,
                      },
                    ]),
              ],
            }}
          >
            <Button icon={<SafetyCertificateOutlined />}>
              证书状态 <Badge status={certStatus?.installed ? 'success' : 'default'} />
            </Button>
          </Dropdown>
          <Button
            icon={<DeleteOutlined />}
            onClick={() => {
              setMessages([])
              setSelected(null)
            }}
          >
            清空
          </Button>
        </Space>
      </div>

      {/* Main content */}
      <Splitter style={{ flex: 1, overflow: 'hidden' }}>
        <Splitter.Panel defaultSize="40%" min="25%" max="70%">
          <Table
            dataSource={messages}
            columns={columns}
            rowKey="id"
            size="small"
            pagination={false}
            scroll={{ y: 'calc(100vh - 140px)' }}
            onRow={(record) => ({
              onClick: () => setSelected(record),
              style: {
                cursor: 'pointer',
                background: selected?.id === record.id ? '#e6f4ff' : undefined,
              },
            })}
          />
        </Splitter.Panel>
        <Splitter.Panel>
          <div style={{ height: '100%', overflow: 'auto', padding: '0 12px' }}>
            {selected ? (
              <Tabs
                defaultActiveKey={isClaudeApiRequest(selected) ? 'claude' : 'request'}
                key={selected.id}
                items={[
                  ...(isClaudeApiRequest(selected)
                    ? [
                        {
                          key: 'claude',
                          label: (
                            <span>
                              <RobotOutlined style={{ marginRight: 4 }} />
                              Claude 对话
                            </span>
                          ),
                          children: <ClaudeMessageViewer message={selected} />,
                        },
                      ]
                    : []),
                  {
                    key: 'request',
                    label: '请求',
                    children: (
                      <div>
                        <div style={{ marginBottom: 8 }}>
                          <Tag color={methodColors[selected.method]}>{selected.method}</Tag>
                          <span style={{ fontSize: 13, wordBreak: 'break-all' }}>
                            {selected.url}
                          </span>
                        </div>
                        <h4>请求头</h4>
                        <HeadersTable headers={selected.request_headers} />
                        <h4 style={{ marginTop: 16 }}>请求体</h4>
                        <BodyViewer body={selected.request_body} label="请求" />
                      </div>
                    ),
                  },
                  {
                    key: 'response',
                    label: `响应 (${selected.status})`,
                    children: (
                      <div>
                        <div style={{ marginBottom: 8 }}>
                          <Tag color={statusColor(selected.status)}>{selected.status}</Tag>
                          <span style={{ fontSize: 13, color: '#999' }}>
                            {selected.duration_ms}ms
                          </span>
                        </div>
                        <h4>响应头</h4>
                        <HeadersTable headers={selected.response_headers} />
                        <h4 style={{ marginTop: 16 }}>响应体</h4>
                        <BodyViewer body={selected.response_body} label="响应" />
                      </div>
                    ),
                  },
                ]}
              />
            ) : (
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  height: '100%',
                }}
              >
                <Empty description="选择一条消息查看详情" />
              </div>
            )}
          </div>
        </Splitter.Panel>
      </Splitter>
    </div>
  )
}
