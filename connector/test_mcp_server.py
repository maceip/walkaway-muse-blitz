import unittest

import mcp_server

STATE = {
    "status": "predicted", "goal": "g", "say": "proposed",
    "page": {"title": "T", "url": "https://x.test", "screenshot": "SECRET_FRAME"},
    "decision": {"operation": "CLICK", "label": "Search", "choice": "e1"},
    "history_count": 2, "elapsed_ms": 10, "decision_model": "muse-spark-1.3",
}


class Rpc(unittest.TestCase):
    def setUp(self):
        self.calls = []
        mcp_server.relay = lambda cmd, body=None: (self.calls.append(cmd), dict(STATE))[1]

    def test_initialize(self):
        result, rid = mcp_server.handle_rpc({"method": "initialize", "id": 1})
        self.assertEqual(result["serverInfo"]["name"], "local-browser-for-muse")
        self.assertIn("cannot approve", result["instructions"])

    def test_tools_list_has_no_approve_tool(self):
        names = [t["name"] for t in mcp_server.handle_rpc({"method": "tools/list", "id": 2})[0]["tools"]]
        self.assertNotIn("walkaway_approve_step", names)
        self.assertIn("walkaway_request_approval", names)
        for tool in mcp_server.TOOLS:
            self.assertIn("readOnlyHint", tool["annotations"])

    def test_status_strips_screenshot(self):
        result, _ = mcp_server.handle_rpc(
            {"method": "tools/call", "id": 3, "params": {"name": "walkaway_status", "arguments": {}}}
        )
        self.assertNotIn("SECRET_FRAME", result["content"][0]["text"])
        self.assertTrue(result["structuredContent"]["awaiting_approval"])
        self.assertTrue(result["structuredContent"]["approval_url"])

    def test_unknown_method_is_rpc_error(self):
        result, rid = mcp_server.handle_rpc({"method": "bogus", "id": 9})
        self.assertEqual(result["_error"]["code"], -32601)


if __name__ == "__main__":
    unittest.main()
