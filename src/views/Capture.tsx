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
  Modal,
  Progress,
} from 'antd'
import {
  PlayCircleOutlined,
  PauseCircleOutlined,
  DeleteOutlined,
  SafetyCertificateOutlined,
  CopyOutlined,
  RobotOutlined,
  DownloadOutlined,
  ToolOutlined,
  CheckCircleOutlined,
  CloudDownloadOutlined,
  GithubOutlined,
} from '@ant-design/icons'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { save } from '@tauri-apps/plugin-dialog'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { CapturedMessage, ProxyStatus, CertStatus, UpdateInfo } from '@/types'
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

function toCurl(msg: CapturedMessage): string {
  const parts = ['curl']
  if (msg.method !== 'GET') {
    parts.push('-X', msg.method)
  }
  parts.push(`'${msg.url}'`)
  for (const [key, value] of Object.entries(msg.request_headers)) {
    parts.push('-H', `'${key}: ${value}'`)
  }
  if (msg.request_body) {
    parts.push('-d', `'${msg.request_body}'`)
  }
  return parts.join(' \\\n  ')
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
  const [mitmdumpInstalled, setMitmdumpInstalled] = useState<boolean | null>(null)
  const [downloading, setDownloading] = useState(false)
  const [downloadProgress, setDownloadProgress] = useState({ downloaded: 0, total: 0, stage: '' })
  const [aboutOpen, setAboutOpen] = useState(false)
  const [appVersion, setAppVersion] = useState('')
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
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

    // Get app version on mount
    invoke<string>('get_app_version').then(setAppVersion)

    // Check mitmdump installation status on mount
    invoke<boolean>('check_mitmdump').then((installed) => {
      setMitmdumpInstalled(installed)
    })

    let unlistenMenu: (() => void) | null = null
    const setup = async () => {
      const unlisten = await listen<CapturedMessage>('captured-message', (event) => {
        setMessages((prev) => [event.payload, ...prev])
      })
      listenerRef.current = unlisten

      unlistenMenu = await listen('menu-about', () => {
        setAboutOpen(true)
        setUpdateInfo(null)
      })
    }
    setup()

    return () => {
      listenerRef.current?.()
      unlistenMenu?.()
    }
  }, [])

  const handleStart = async () => {
    try {
      // Check if mitmdump is available
      const exists = await invoke<boolean>('check_mitmdump')

      if (!exists) {
        // Download mitmdump with progress tracking
        setDownloading(true)
        setDownloadProgress({ downloaded: 0, total: 0, stage: 'downloading' })

        const unlisten = await listen<{ downloaded: number; total: number; stage: string }>(
          'mitmdump-download-progress',
          (event) => {
            setDownloadProgress(event.payload)
          }
        )

        try {
          await invoke('download_mitmdump')
          setMitmdumpInstalled(true)
          message.success('mitmproxy 下载完成')
        } catch (e) {
          message.error(`下载失败: ${e}`)
          return
        } finally {
          unlisten()
          setDownloading(false)
        }
      }

      // Start proxy
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

  const handleInstallMitmdump = async () => {
    setDownloading(true)
    setDownloadProgress({ downloaded: 0, total: 0, stage: 'downloading' })

    const unlisten = await listen<{ downloaded: number; total: number; stage: string }>(
      'mitmdump-download-progress',
      (event) => {
        setDownloadProgress(event.payload)
      }
    )

    try {
      await invoke('download_mitmdump')
      setMitmdumpInstalled(true)
      message.success('mitmproxy 安装完成')
    } catch (e) {
      message.error(`安装失败: ${e}`)
    } finally {
      unlisten()
      setDownloading(false)
    }
  }

  const handleUninstallMitmdump = async () => {
    try {
      await invoke('uninstall_mitmdump')
      setMitmdumpInstalled(false)
      message.success('mitmproxy 已卸载')
    } catch (e) {
      message.error(`卸载失败: ${e}`)
    }
  }

  const handleCheckMitmdump = async () => {
    try {
      const installed = await invoke<boolean>('check_mitmdump')
      setMitmdumpInstalled(installed)
      if (installed) {
        message.success('mitmproxy 已安装')
      } else {
        message.warning('mitmproxy 未安装')
      }
    } catch (e) {
      message.error(`检查失败: ${e}`)
    }
  }

  const handleCopyProxyCommand = async () => {
    try {
      const certPath = await invoke<string>('get_ca_cert_path')
      const cmd = `export http_proxy=http://127.0.0.1:${port} && export https_proxy=http://127.0.0.1:${port} && export no_proxy=localhost,127.0.0.1,::1 && export NODE_EXTRA_CA_CERTS=${certPath}`
      await writeText(cmd)
      message.success('代理命令已复制，请在终端中粘贴执行')
    } catch (e) {
      message.error(`复制失败: ${e}`)
    }
  }

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true)
    setUpdateInfo(null)
    try {
      const info = await invoke<UpdateInfo>('check_update')
      setUpdateInfo(info)
    } catch (e) {
      message.error(`检查更新失败: ${e}`)
    } finally {
      setCheckingUpdate(false)
    }
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
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
              loading={downloading}
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
                  label: mitmdumpInstalled ? '已安装' : '未安装',
                  onClick: handleCheckMitmdump,
                },
                { type: 'divider' },
                ...(mitmdumpInstalled
                  ? [
                      {
                        key: 'uninstall',
                        label: '卸载 mitmproxy',
                        onClick: handleUninstallMitmdump,
                        disabled: running,
                      },
                    ]
                  : [
                      {
                        key: 'install',
                        label: '安装 mitmproxy',
                        onClick: handleInstallMitmdump,
                        disabled: downloading,
                      },
                    ]),
              ],
            }}
          >
            <Button icon={<ToolOutlined />}>
              mitmproxy <Badge status={mitmdumpInstalled ? 'success' : 'default'} />
            </Button>
          </Dropdown>
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
          <Button icon={<CopyOutlined />} onClick={handleCopyProxyCommand}>
            复制代理命令
          </Button>
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
          <Dropdown
            menu={{
              items: [
                {
                  key: 'copy-curl',
                  icon: <CopyOutlined />,
                  label: '复制为 cURL',
                  onClick: () => {
                    if (selected) {
                      navigator.clipboard.writeText(toCurl(selected))
                      message.success('cURL 已复制')
                    }
                  },
                },
                {
                  key: 'copy-url',
                  icon: <CopyOutlined />,
                  label: '复制 URL',
                  onClick: () => {
                    if (selected) {
                      navigator.clipboard.writeText(selected.url)
                      message.success('URL 已复制')
                    }
                  },
                },
              ],
            }}
            trigger={['contextMenu']}
          >
            <div>
              <Table
                dataSource={messages}
                columns={columns}
                rowKey="id"
                size="small"
                pagination={false}
                scroll={{ y: 'calc(100vh - 140px)' }}
                onRow={(record) => ({
                  onClick: () => setSelected(record),
                  onContextMenu: () => setSelected(record),
                  style: {
                    cursor: 'pointer',
                    background: selected?.id === record.id ? '#e6f4ff' : undefined,
                  },
                })}
              />
            </div>
          </Dropdown>
        </Splitter.Panel>
        <Splitter.Panel>
          <div style={{ height: '100%', overflow: 'hidden', padding: '0 12px', display: 'flex', flexDirection: 'column' }}>
            {selected ? (
              <Tabs
                defaultActiveKey={isClaudeApiRequest(selected) ? 'claude' : 'request'}
                key={selected.id}
                style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}
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

      <Modal
        title="关于"
        open={aboutOpen}
        onCancel={() => setAboutOpen(false)}
        footer={null}
        width={480}
      >
        <div style={{ textAlign: 'center', padding: '16px 0 8px' }}>
          <h2 style={{ margin: '0 0 4px' }}>CC Message Capture</h2>
          <p style={{ color: '#999', margin: '0 0 16px' }}>当前版本: v{appVersion}</p>
          <Button
            type="primary"
            icon={<CloudDownloadOutlined />}
            loading={checkingUpdate}
            onClick={handleCheckUpdate}
          >
            检查更新
          </Button>
        </div>

        {updateInfo && (
          <div style={{ marginTop: 16 }}>
            {updateInfo.has_update ? (
              <>
                <div style={{ background: '#f6ffed', border: '1px solid #b7eb8f', borderRadius: 6, padding: 12, marginBottom: 12 }}>
                  <p style={{ margin: 0, fontWeight: 500 }}>
                    发现新版本: v{updateInfo.latest_version}
                  </p>
                  {updateInfo.release_notes && (
                    <p style={{ margin: '8px 0 0', fontSize: 13, color: '#666', whiteSpace: 'pre-wrap' }}>
                      {updateInfo.release_notes}
                    </p>
                  )}
                </div>
                {updateInfo.assets.length > 0 && (
                  <div style={{ marginBottom: 12 }}>
                    <p style={{ margin: '0 0 8px', fontWeight: 500, fontSize: 13 }}>下载安装包:</p>
                    <Space direction="vertical" style={{ width: '100%' }}>
                      {updateInfo.assets.map((asset) => (
                        <Button
                          key={asset.name}
                          icon={<DownloadOutlined />}
                          block
                          onClick={() => openUrl(asset.download_url)}
                        >
                          {asset.name} ({formatSize(asset.size)})
                        </Button>
                      ))}
                    </Space>
                  </div>
                )}
                <div style={{ textAlign: 'center' }}>
                  <Button
                    type="link"
                    icon={<GithubOutlined />}
                    onClick={() => openUrl(updateInfo.release_url)}
                  >
                    前往 GitHub Release 页面
                  </Button>
                </div>
              </>
            ) : (
              <div style={{ textAlign: 'center', padding: '8px 0' }}>
                <CheckCircleOutlined style={{ fontSize: 24, color: '#52c41a', marginBottom: 8 }} />
                <p style={{ margin: 0, color: '#666' }}>当前已是最新版本</p>
              </div>
            )}
          </div>
        )}
      </Modal>

      <Modal
        title="正在下载 mitmproxy"
        open={downloading}
        closable={false}
        footer={
          downloadProgress.stage !== 'extracting' ? (
            <Button
              onClick={async () => {
                try {
                  await invoke('cancel_download')
                } catch (e) {
                  message.error(`取消失败: ${e}`)
                }
              }}
            >
              取消
            </Button>
          ) : null
        }
        maskClosable={false}
      >
        <div style={{ textAlign: 'center', padding: '20px 0' }}>
          <DownloadOutlined style={{ fontSize: 32, color: '#1890ff', marginBottom: 16 }} />
          <Progress
            percent={
              downloadProgress.total > 0
                ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100)
                : 0
            }
            status="active"
          />
          <p style={{ marginTop: 12, color: '#666' }}>
            {downloadProgress.stage === 'extracting'
              ? '正在解压...'
              : downloadProgress.total > 0
                ? `${(downloadProgress.downloaded / 1024 / 1024).toFixed(1)} / ${(downloadProgress.total / 1024 / 1024).toFixed(1)} MB`
                : '正在连接...'}
          </p>
        </div>
      </Modal>
    </div>
  )
}
