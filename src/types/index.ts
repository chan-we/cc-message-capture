export interface CapturedMessage {
  id: string
  timestamp: string
  method: string
  url: string
  request_headers: Record<string, string>
  request_body: string
  status: number
  response_headers: Record<string, string>
  response_body: string
  duration_ms: number
}

export interface ProxyStatus {
  running: boolean
  port: number
}

export interface CertStatus {
  installed: boolean
  method: string
  details: string
}

export interface ReleaseAsset {
  name: string
  download_url: string
  size: number
}

export interface UpdateInfo {
  has_update: boolean
  current_version: string
  latest_version: string
  release_url: string
  release_notes: string
  assets: ReleaseAsset[]
}

// Claude SSE / Message types

export interface SSEEvent {
  event: string
  data: string
  parsedData?: any
  id?: string
  timestamp: number
}

export interface ContentBlock {
  type: 'thinking' | 'text' | 'tool_use' | 'tool_result'
  content: string
  id?: string
  name?: string
  input?: string
  signature?: string
  is_error?: boolean
}

export interface TokenUsage {
  input_tokens: number
  output_tokens: number
  cache_read_input_tokens?: number
  cache_creation_input_tokens?: number
}

export interface ParsedClaudeResponse {
  model?: string
  role?: string
  blocks: ContentBlock[]
  usage?: TokenUsage
  stop_reason?: string
}

export interface ClaudeRequestMessage {
  role: 'user' | 'assistant'
  content: string | ClaudeRequestPart[]
}

export interface ClaudeRequestPart {
  type: string
  text?: string
  thinking?: string
  id?: string
  name?: string
  input?: any
  content?: string | any[]
  tool_use_id?: string
  is_error?: boolean
  signature?: string
}

export interface ParsedClaudeRequest {
  model?: string
  max_tokens?: number
  messages: ClaudeRequestMessage[]
  system?: any[]
  tools?: any[]
  stream?: boolean
}
