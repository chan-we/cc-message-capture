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
