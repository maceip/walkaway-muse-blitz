# Muse connector service

Canonical working source:
`/Users/mac/walkaway-hackathon/connector/mcp_server.py`

Deployed endpoint: `https://muse.secure.build/mcp`

Boundary:

- Exposes Walkaway task tools to Muse over MCP streamable HTTP.
- Authenticates with the connector bearer token.
- Has no approve tool.
- Does not receive screenshots.
