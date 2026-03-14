import type {
  SSEEvent,
  ContentBlock,
  ParsedClaudeResponse,
  ParsedClaudeRequest,
  CapturedMessage,
} from '@/types'

/**
 * Check if a captured message is a Claude API request
 */
export function isClaudeApiRequest(msg: CapturedMessage): boolean {
  const url = msg.url.toLowerCase()
  return (
    url.includes('/v1/messages') ||
    url.includes('api.anthropic.com') ||
    url.includes('api.claude.ai')
  )
}

/**
 * Parse raw SSE text into structured events
 */
export function parseSSE(rawText: string): SSEEvent[] {
  const lines = rawText.split('\n')
  const events: SSEEvent[] = []
  let currentEvent: Partial<SSEEvent> = {}
  let started = false

  for (const line of lines) {
    const trimmed = line.trim()

    if (!started && (trimmed.startsWith('event:') || trimmed.startsWith('data:'))) {
      started = true
    }
    if (!started) continue

    if (trimmed === '') {
      if (currentEvent.event || currentEvent.data) {
        try {
          if (currentEvent.data) {
            currentEvent.parsedData = JSON.parse(currentEvent.data)
          }
        } catch {
          // keep as string
        }
        events.push({
          event: currentEvent.event || 'message',
          data: currentEvent.data || '',
          parsedData: currentEvent.parsedData,
          timestamp: Date.now(),
          id: currentEvent.id,
        })
        currentEvent = {}
      }
      continue
    }

    if (trimmed.startsWith('event:')) {
      currentEvent.event = trimmed.slice(6).trim()
    } else if (trimmed.startsWith('data:')) {
      const dataPart = trimmed.slice(5).trim()
      currentEvent.data = (currentEvent.data || '') + dataPart
    } else if (trimmed.startsWith('id:')) {
      currentEvent.id = trimmed.slice(3).trim()
    }
  }

  // Handle last block
  if (currentEvent.event || currentEvent.data) {
    try {
      if (currentEvent.data) {
        currentEvent.parsedData = JSON.parse(currentEvent.data)
      }
    } catch {}
    events.push({
      event: currentEvent.event || 'message',
      data: currentEvent.data || '',
      parsedData: currentEvent.parsedData,
      timestamp: Date.now(),
      id: currentEvent.id,
    })
  }

  return events
}

/**
 * Reconstruct a complete message state from SSE events
 */
export function reconstructMessage(events: SSEEvent[]): ParsedClaudeResponse {
  const state: ParsedClaudeResponse = {
    blocks: [],
    usage: undefined,
    model: undefined,
    stop_reason: undefined,
  }

  for (const ev of events) {
    const data = ev.parsedData
    if (!data) continue

    switch (data.type) {
      case 'message_start':
        state.model = data.message?.model
        state.role = data.message?.role
        if (data.message?.usage) {
          state.usage = { ...data.message.usage }
        }
        break

      case 'content_block_start': {
        const block: ContentBlock = {
          type: data.content_block?.type || 'text',
          content: '',
          id: data.content_block?.id,
          name: data.content_block?.name,
          input: '',
          signature: '',
        }
        state.blocks[data.index] = block
        break
      }

      case 'content_block_delta': {
        const block = state.blocks[data.index]
        if (!block) break
        const delta = data.delta
        if (!delta) break

        if (delta.type === 'thinking_delta') {
          block.content += delta.thinking || ''
        } else if (delta.type === 'text_delta') {
          block.content += delta.text || ''
        } else if (delta.type === 'input_json_delta') {
          block.input = (block.input || '') + (delta.partial_json || '')
        } else if (delta.type === 'signature_delta') {
          block.signature = delta.signature
        }
        break
      }

      case 'message_delta':
        state.stop_reason = data.delta?.stop_reason
        if (data.usage) {
          state.usage = { ...state.usage, ...data.usage } as any
        }
        break
    }
  }

  // Try to parse tool_use input as JSON
  for (const block of state.blocks) {
    if (block?.type === 'tool_use' && block.input && typeof block.input === 'string') {
      try {
        const parsed = JSON.parse(block.input)
        block.input = JSON.stringify(parsed, null, 2)
      } catch {
        // keep as-is
      }
    }
  }

  return state
}

/**
 * Parse the request body of a Claude API call
 */
export function parseClaudeRequest(body: string): ParsedClaudeRequest | null {
  try {
    const parsed = JSON.parse(body)
    if (parsed.messages && Array.isArray(parsed.messages)) {
      return parsed as ParsedClaudeRequest
    }
    return null
  } catch {
    return null
  }
}

/**
 * Parse Claude response - handles both SSE streaming and JSON responses
 */
export function parseClaudeResponse(msg: CapturedMessage): ParsedClaudeResponse | null {
  const contentType = Object.entries(msg.response_headers).find(
    ([k]) => k.toLowerCase() === 'content-type'
  )?.[1]

  const body = msg.response_body
  if (!body) return null

  // SSE streaming response
  if (contentType?.includes('text/event-stream') || body.trimStart().startsWith('event:')) {
    const events = parseSSE(body)
    if (events.length > 0) {
      return reconstructMessage(events)
    }
  }

  // Non-streaming JSON response
  try {
    const json = JSON.parse(body)
    if (json.content && Array.isArray(json.content)) {
      const blocks: ContentBlock[] = json.content.map((c: any) => ({
        type: c.type,
        content: c.text || c.thinking || '',
        id: c.id,
        name: c.name,
        input: c.input ? JSON.stringify(c.input, null, 2) : undefined,
        signature: c.signature,
      }))
      return {
        model: json.model,
        role: json.role,
        blocks,
        usage: json.usage,
        stop_reason: json.stop_reason,
      }
    }
  } catch {
    // not JSON
  }

  return null
}
