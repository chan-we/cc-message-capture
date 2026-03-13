"""
mitmproxy addon: capture HTTP flows and emit as JSON Lines to stdout.
Each line is a complete JSON object matching the CapturedMessage struct.
"""
import json
import sys
import uuid
from datetime import datetime, timezone

from mitmproxy import http


class CaptureAddon:
    def response(self, flow: http.HTTPFlow):
        if flow.request.method == "CONNECT":
            return

        request = flow.request
        response = flow.response
        if response is None:
            return

        # Request headers (last value wins for duplicate keys)
        req_headers = {}
        for k, v in request.headers.items(multi=True):
            req_headers[k] = v

        # Response headers
        resp_headers = {}
        for k, v in response.headers.items(multi=True):
            resp_headers[k] = v

        # Request body
        req_body = ""
        if request.content:
            try:
                req_body = request.content.decode("utf-8", errors="replace")
            except Exception:
                req_body = "<binary>"

        # Response body
        resp_body = ""
        if response.content:
            try:
                resp_body = response.content.decode("utf-8", errors="replace")
            except Exception:
                resp_body = "<binary>"

        # Duration in milliseconds
        duration_ms = 0
        if request.timestamp_start and response.timestamp_end:
            duration_ms = int(
                (response.timestamp_end - request.timestamp_start) * 1000
            )

        # Timestamp in RFC3339/ISO format
        timestamp = datetime.fromtimestamp(
            request.timestamp_start, tz=timezone.utc
        ).isoformat()

        msg = {
            "id": str(uuid.uuid4()),
            "timestamp": timestamp,
            "method": request.method,
            "url": request.pretty_url,
            "request_headers": req_headers,
            "request_body": req_body,
            "status": response.status_code,
            "response_headers": resp_headers,
            "response_body": resp_body,
            "duration_ms": duration_ms,
        }

        line = json.dumps(msg, ensure_ascii=False)
        sys.stdout.write(line + "\n")
        sys.stdout.flush()


addons = [CaptureAddon()]
