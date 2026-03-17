import { useMemo } from 'react'
import { Tag, Tabs, Empty, Descriptions, Statistic, Space, Typography, Tooltip } from 'antd'
import {
  ThunderboltOutlined,
  MessageOutlined,
  ToolOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  BulbOutlined,
  UserOutlined,
  RobotOutlined,
  CopyOutlined,
} from '@ant-design/icons'
import { message as antMessage } from 'antd'
import type {
  CapturedMessage,
  ContentBlock,
  ClaudeRequestPart,
  ClaudeRequestMessage,
} from '@/types'
import {
  isClaudeApiRequest,
  parseClaudeResponse,
  parseClaudeRequest,
} from '@/services/claudeParser'

const { Text, Paragraph } = Typography

// -- Style constants --

const blockStyles: Record<string, React.CSSProperties> = {
  thinking: {
    background: '#fffbe6',
    border: '1px solid #ffe58f',
    borderRadius: 8,
    marginBottom: 12,
  },
  text: {
    background: '#f6ffed',
    border: '1px solid #b7eb8f',
    borderRadius: 8,
    marginBottom: 12,
  },
  tool_use: {
    background: '#e6f4ff',
    border: '1px solid #91caff',
    borderRadius: 8,
    marginBottom: 12,
  },
  tool_result: {
    background: '#f6ffed',
    border: '1px solid #b7eb8f',
    borderRadius: 8,
    marginBottom: 12,
  },
  tool_result_error: {
    background: '#fff2f0',
    border: '1px solid #ffccc7',
    borderRadius: 8,
    marginBottom: 12,
  },
}

const blockHeaderStyles: Record<string, React.CSSProperties> = {
  thinking: {
    background: '#fffbe6',
    borderBottom: '1px solid #ffe58f',
    padding: '6px 12px',
    borderRadius: '8px 8px 0 0',
  },
  text: {
    background: '#f6ffed',
    borderBottom: '1px solid #b7eb8f',
    padding: '6px 12px',
    borderRadius: '8px 8px 0 0',
  },
  tool_use: {
    background: '#e6f4ff',
    borderBottom: '1px solid #91caff',
    padding: '6px 12px',
    borderRadius: '8px 8px 0 0',
  },
  tool_result: {
    background: '#f6ffed',
    borderBottom: '1px solid #b7eb8f',
    padding: '6px 12px',
    borderRadius: '8px 8px 0 0',
  },
}

const blockIcons: Record<string, React.ReactNode> = {
  thinking: <BulbOutlined style={{ color: '#faad14' }} />,
  text: <MessageOutlined style={{ color: '#52c41a' }} />,
  tool_use: <ToolOutlined style={{ color: '#1677ff' }} />,
  tool_result: <CheckCircleOutlined style={{ color: '#52c41a' }} />,
}

const blockTagColors: Record<string, string> = {
  thinking: 'warning',
  text: 'success',
  tool_use: 'processing',
  tool_result: 'success',
}

const blockLabels: Record<string, string> = {
  thinking: '思考过程',
  text: '文本回复',
  tool_use: '工具调用',
  tool_result: '工具结果',
}

// -- Sub-components --

function CopyButton({ text }: { text: string }) {
  return (
    <Tooltip title="复制">
      <CopyOutlined
        style={{ cursor: 'pointer', color: '#999', fontSize: 12 }}
        onClick={(e) => {
          e.stopPropagation()
          navigator.clipboard.writeText(text)
          antMessage.success('已复制')
        }}
      />
    </Tooltip>
  )
}

function ContentBlockView({ block, index }: { block: ContentBlock; index: number }) {
  const isError = block.is_error
  const styleKey = isError ? 'tool_result_error' : block.type

  return (
    <div style={blockStyles[styleKey] || blockStyles.text}>
      {/* Block header */}
      <div
        style={{
          ...(blockHeaderStyles[block.type] || blockHeaderStyles.text),
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}
      >
        <Space size={6}>
          {isError ? (
            <CloseCircleOutlined style={{ color: '#ff4d4f' }} />
          ) : (
            blockIcons[block.type] || <MessageOutlined />
          )}
          <Tag color={isError ? 'error' : blockTagColors[block.type]} style={{ margin: 0 }}>
            {blockLabels[block.type] || block.type}
          </Tag>
          {block.name && (
            <Text code style={{ fontSize: 11 }}>
              {block.name}
            </Text>
          )}
          {block.id && (
            <Text type="secondary" style={{ fontSize: 10 }}>
              {block.id}
            </Text>
          )}
        </Space>
        <Space size={8}>
          <Text type="secondary" style={{ fontSize: 10 }}>
            #{index}
          </Text>
          {block.content && <CopyButton text={block.content} />}
        </Space>
      </div>

      {/* Block body */}
      <div style={{ padding: 12 }}>
        {block.type === 'thinking' && (
          <div>
            <Paragraph
              style={{
                whiteSpace: 'pre-wrap',
                fontSize: 13,
                color: '#8c8c00',
                fontStyle: 'italic',
                lineHeight: 1.7,
                margin: 0,
                maxHeight: 400,
                overflow: 'auto',
              }}
            >
              {block.content}
            </Paragraph>
            {block.signature && (
              <div
                style={{
                  marginTop: 8,
                  paddingTop: 8,
                  borderTop: '1px dashed #ffe58f',
                  fontSize: 10,
                  color: '#bfbf00',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                签名: {block.signature}
              </div>
            )}
          </div>
        )}

        {block.type === 'text' && (
          <Paragraph
            style={{
              whiteSpace: 'pre-wrap',
              fontSize: 13,
              lineHeight: 1.7,
              margin: 0,
            }}
          >
            {block.content}
          </Paragraph>
        )}

        {block.type === 'tool_use' && (
          <div>
            <pre
              style={{
                background: '#f0f5ff',
                padding: 10,
                borderRadius: 6,
                fontSize: 12,
                lineHeight: 1.5,
                overflow: 'auto',
                maxHeight: 400,
                margin: 0,
                color: '#1d39c4',
              }}
            >
              {block.input || '{}'}
            </pre>
          </div>
        )}

        {block.type === 'tool_result' && (
          <Paragraph
            style={{
              whiteSpace: 'pre-wrap',
              fontSize: 12,
              lineHeight: 1.6,
              margin: 0,
              fontFamily: 'monospace',
              color: isError ? '#cf1322' : undefined,
            }}
          >
            {block.content}
          </Paragraph>
        )}
      </div>
    </div>
  )
}

function RequestPartView({ part }: { part: ClaudeRequestPart }) {
  switch (part.type) {
    case 'text':
      return (
        <Paragraph
          style={{
            whiteSpace: 'pre-wrap',
            fontSize: 13,
            lineHeight: 1.7,
            margin: 0,
            marginBottom: 8,
          }}
        >
          {part.text}
        </Paragraph>
      )
    case 'thinking':
      return (
        <div style={{ ...blockStyles.thinking, padding: 10 }}>
          <Space size={4} style={{ marginBottom: 6 }}>
            <BulbOutlined style={{ color: '#faad14' }} />
            <Tag color="warning" style={{ margin: 0 }}>
              思考
            </Tag>
          </Space>
          <Paragraph
            style={{
              whiteSpace: 'pre-wrap',
              fontSize: 12,
              fontStyle: 'italic',
              color: '#8c8c00',
              margin: 0,
              maxHeight: 300,
              overflow: 'auto',
            }}
          >
            {part.thinking}
          </Paragraph>
        </div>
      )
    case 'tool_use':
      return (
        <div style={{ ...blockStyles.tool_use, padding: 10 }}>
          <Space size={4} style={{ marginBottom: 6 }}>
            <ToolOutlined style={{ color: '#1677ff' }} />
            <Tag color="processing" style={{ margin: 0 }}>
              工具调用: {part.name}
            </Tag>
            {part.id && (
              <Text type="secondary" style={{ fontSize: 10 }}>
                {part.id}
              </Text>
            )}
          </Space>
          <pre
            style={{
              background: '#f0f5ff',
              padding: 8,
              borderRadius: 6,
              fontSize: 11,
              overflow: 'auto',
              maxHeight: 300,
              margin: 0,
              color: '#1d39c4',
            }}
          >
            {JSON.stringify(part.input, null, 2)}
          </pre>
        </div>
      )
    case 'tool_result': {
      const isError = part.is_error
      return (
        <div style={{ ...(isError ? blockStyles.tool_result_error : blockStyles.tool_result), padding: 10 }}>
          <Space size={4} style={{ marginBottom: 6 }}>
            {isError ? (
              <CloseCircleOutlined style={{ color: '#ff4d4f' }} />
            ) : (
              <CheckCircleOutlined style={{ color: '#52c41a' }} />
            )}
            <Tag color={isError ? 'error' : 'success'} style={{ margin: 0 }}>
              工具结果
            </Tag>
            {part.tool_use_id && (
              <Text type="secondary" style={{ fontSize: 10 }}>
                {part.tool_use_id}
              </Text>
            )}
          </Space>
          <Paragraph
            style={{
              whiteSpace: 'pre-wrap',
              fontSize: 12,
              fontFamily: 'monospace',
              margin: 0,
              maxHeight: 300,
              overflow: 'auto',
              color: isError ? '#cf1322' : undefined,
            }}
          >
            {typeof part.content === 'string'
              ? part.content
              : JSON.stringify(part.content, null, 2)}
          </Paragraph>
        </div>
      )
    }
    default:
      return (
        <Text type="secondary" style={{ fontSize: 11, fontStyle: 'italic' }}>
          未知类型: {part.type}
        </Text>
      )
  }
}

function MessageBubble({ msg, index }: { msg: ClaudeRequestMessage; index: number }) {
  const isUser = msg.role === 'user'
  const bgColor = isUser ? '#fafafa' : '#f0f5ff'
  const borderColor = isUser ? '#d9d9d9' : '#adc6ff'

  const parts: ClaudeRequestPart[] =
    typeof msg.content === 'string'
      ? [{ type: 'text', text: msg.content }]
      : msg.content

  return (
    <div
      style={{
        border: `1px solid ${borderColor}`,
        borderRadius: 10,
        background: bgColor,
        marginBottom: 16,
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          padding: '6px 12px',
          background: isUser ? '#f5f5f5' : '#e6f4ff',
          borderBottom: `1px solid ${borderColor}`,
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        {isUser ? (
          <UserOutlined style={{ color: '#595959' }} />
        ) : (
          <RobotOutlined style={{ color: '#1677ff' }} />
        )}
        <Text strong style={{ fontSize: 12 }}>
          {isUser ? '用户' : '助手'}
        </Text>
        <Text type="secondary" style={{ fontSize: 10, marginLeft: 'auto' }}>
          #{index}
        </Text>
      </div>
      <div style={{ padding: 12 }}>
        {parts.map((part, i) => (
          <RequestPartView key={i} part={part} />
        ))}
      </div>
    </div>
  )
}

// -- Main component --

interface ClaudeMessageViewerProps {
  message: CapturedMessage
}

export default function ClaudeMessageViewer({ message }: ClaudeMessageViewerProps) {
  const isClaudeApi = useMemo(() => isClaudeApiRequest(message), [message])
  const parsedResponse = useMemo(
    () => (isClaudeApi ? parseClaudeResponse(message) : null),
    [message, isClaudeApi]
  )
  const parsedRequest = useMemo(
    () => (isClaudeApi ? parseClaudeRequest(message.request_body) : null),
    [message, isClaudeApi]
  )

  if (!isClaudeApi) {
    return <Empty description="非 Claude API 请求" />
  }

  if (!parsedResponse && !parsedRequest) {
    return <Empty description="无法解析 Claude 消息" />
  }

  return (
    <div style={{ padding: '4px 0' }}>
      {/* Response info header */}
      {parsedResponse && (
        <div style={{ marginBottom: 16 }}>
          <Descriptions
            size="small"
            column={4}
            bordered
            style={{ marginBottom: 12 }}
            items={[
              {
                key: 'model',
                label: '模型',
                children: (
                  <Tag color="blue">{parsedResponse.model || '未知'}</Tag>
                ),
              },
              {
                key: 'stop',
                label: '结束原因',
                children: (
                  <Tag color={parsedResponse.stop_reason === 'end_turn' ? 'green' : 'default'}>
                    {parsedResponse.stop_reason || '流式传输中...'}
                  </Tag>
                ),
              },
              {
                key: 'blocks',
                label: '内容块数',
                children: parsedResponse.blocks.length,
              },
              {
                key: 'status',
                label: '状态码',
                children: <Tag color="green">{message.status}</Tag>,
              },
            ]}
          />

          {/* Token usage */}
          {parsedResponse.usage && (
            <div
              style={{
                display: 'flex',
                gap: 16,
                padding: '8px 16px',
                background: '#fafafa',
                borderRadius: 8,
                border: '1px solid #f0f0f0',
                marginBottom: 16,
              }}
            >
              <Statistic
                title="输入 Token"
                value={parsedResponse.usage.input_tokens}
                valueStyle={{ fontSize: 16 }}
              />
              <Statistic
                title="输出 Token"
                value={parsedResponse.usage.output_tokens}
                valueStyle={{ fontSize: 16 }}
              />
              {parsedResponse.usage.cache_read_input_tokens != null && (
                <Statistic
                  title="缓存读取"
                  value={parsedResponse.usage.cache_read_input_tokens}
                  valueStyle={{ fontSize: 16, color: '#1677ff' }}
                />
              )}
              {parsedResponse.usage.cache_creation_input_tokens != null && (
                <Statistic
                  title="缓存创建"
                  value={parsedResponse.usage.cache_creation_input_tokens}
                  valueStyle={{ fontSize: 16, color: '#faad14' }}
                />
              )}
            </div>
          )}
        </div>
      )}

      {/* Tabs for response, history, system, tools */}
      <Tabs
        defaultActiveKey="response"
        items={[
          ...(parsedResponse && parsedResponse.blocks.length > 0
            ? [
                {
                  key: 'response',
                  label: (
                    <Space size={4}>
                      <RobotOutlined />
                      <span>助手响应</span>
                      <Tag style={{ margin: 0 }}>{parsedResponse.blocks.length}</Tag>
                    </Space>
                  ),
                  children: (
                    <div>
                      {parsedResponse.blocks
                        .filter(Boolean)
                        .map((block, idx) => (
                          <ContentBlockView key={idx} block={block} index={idx} />
                        ))}
                    </div>
                  ),
                },
              ]
            : []),
          ...(parsedRequest && parsedRequest.messages.length > 0
            ? [
                {
                  key: 'messages',
                  label: (
                    <Space size={4}>
                      <ThunderboltOutlined />
                      <span>请求对话历史</span>
                      <Tag style={{ margin: 0 }}>{parsedRequest.messages.length}</Tag>
                    </Space>
                  ),
                  children: (
                    <div>
                      {parsedRequest.messages.map((msg, idx) => (
                        <MessageBubble key={idx} msg={msg} index={idx} />
                      ))}
                    </div>
                  ),
                },
              ]
            : []),
          ...(parsedRequest?.system && parsedRequest.system.length > 0
            ? [
                {
                  key: 'system',
                  label: (
                    <Space size={4}>
                      <span>系统指令</span>
                      <Tag style={{ margin: 0 }}>{parsedRequest.system.length}</Tag>
                    </Space>
                  ),
                  children: (
                    <div>
                      {parsedRequest.system.map((item: any, i: number) => (
                        <div
                          key={i}
                          style={{
                            background: '#fafafa',
                            border: '1px solid #f0f0f0',
                            borderRadius: 8,
                            padding: 12,
                            marginBottom: 8,
                          }}
                        >
                          <Tag style={{ marginBottom: 6 }}>{item.type || 'text'}</Tag>
                          <Paragraph
                            style={{
                              whiteSpace: 'pre-wrap',
                              fontSize: 12,
                              lineHeight: 1.6,
                              margin: 0,
                              maxHeight: 200,
                              overflow: 'auto',
                            }}
                          >
                            {item.text || JSON.stringify(item, null, 2)}
                          </Paragraph>
                        </div>
                      ))}
                    </div>
                  ),
                },
              ]
            : []),
          ...(parsedRequest?.tools && parsedRequest.tools.length > 0
            ? [
                {
                  key: 'tools',
                  label: (
                    <Space size={4}>
                      <ToolOutlined />
                      <span>可用工具</span>
                      <Tag style={{ margin: 0 }}>{parsedRequest.tools.length}</Tag>
                    </Space>
                  ),
                  children: (
                    <div>
                      {parsedRequest.tools.map((tool: any, i: number) => (
                        <div
                          key={i}
                          style={{
                            background: '#f0f5ff',
                            border: '1px solid #d6e4ff',
                            borderRadius: 8,
                            padding: 10,
                            marginBottom: 8,
                          }}
                        >
                          <Space style={{ marginBottom: 4 }}>
                            <Text code style={{ fontSize: 12, fontWeight: 600 }}>
                              {tool.name}
                            </Text>
                          </Space>
                          <Paragraph
                            type="secondary"
                            style={{ fontSize: 11, margin: 0 }}
                            ellipsis={{ rows: 2 }}
                          >
                            {tool.description}
                          </Paragraph>
                        </div>
                      ))}
                    </div>
                  ),
                },
              ]
            : []),
        ]}
      />
    </div>
  )
}
