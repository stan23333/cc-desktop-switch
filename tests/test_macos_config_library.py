import json
import os
import tempfile
import unittest
from unittest.mock import patch

from backend import registry


class MacosConfigLibraryTests(unittest.TestCase):
    def test_apply_config_creates_config_library_entry_when_missing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            json_path = os.path.join(temp_dir, "Claude-3p", "claude_desktop_config.json")
            library_dir = os.path.join(os.path.dirname(json_path), "configLibrary")

            with patch.object(registry, "MAC_3P_CONFIG", json_path):
                with patch("backend.registry._mac_apply_plist_config", return_value={"success": True}):
                    result = registry._mac_apply_config(
                        "http://127.0.0.1:18080",
                        gateway_api_key="secret-value",
                        inference_models='[{"name":"model-a","displayName":"Model A"},{"name":"model-b","supports1m":true}]',
                        auth_scheme="x-api-key",
                        gateway_headers='["x-api-key: secret-value"]',
                    )

            self.assertTrue(result["success"])
            with open(os.path.join(library_dir, "_meta.json"), encoding="utf-8") as handle:
                meta = json.load(handle)
            entry_id = meta["appliedId"]
            self.assertTrue(entry_id)
            with open(os.path.join(library_dir, f"{entry_id}.json"), encoding="utf-8") as handle:
                saved = json.load(handle)

        self.assertEqual(saved["inferenceProvider"], "gateway")
        self.assertEqual(saved["inferenceGatewayBaseUrl"], "http://127.0.0.1:18080")
        self.assertEqual(saved["inferenceGatewayApiKey"], "secret-value")
        self.assertEqual(saved["inferenceGatewayAuthScheme"], "x-api-key")
        self.assertEqual(saved["inferenceGatewayHeaders"], ["x-api-key: secret-value"])
        self.assertEqual(saved["inferenceModels"], ["model-a", "model-b"])
        self.assertIs(saved["isClaudeCodeForDesktopEnabled"], True)


if __name__ == "__main__":
    unittest.main()
