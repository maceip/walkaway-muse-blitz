package com.example.walkawaybridge

import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

data class BusMessage(
  val id: Long,
  val title: String,
  val body: String,
  val kind: String,
  val imageUrl: String? = null,
  val price: String? = null,
  val detail: String? = null,
  val imageB64: String? = null
)

class BusClient(private val baseUrl: String, private val token: String) {
  fun nextMessage(afterId: Long): BusMessage? {
    val connection = open("/messages/next?after=$afterId", "GET")
    val text = readResponse(connection)
    val root = JSONObject(text)
    if (root.isNull("message")) return null
    val m = root.getJSONObject("message")
    return BusMessage(
      id = m.getLong("id"),
      title = m.optString("title", "Walkaway"),
      body = m.optString("body"),
      kind = m.optString("kind", "message"),
      imageUrl = m.optString("image_url", "").takeIf { it.isNotBlank() },
      price = m.optString("price", "").takeIf { it.isNotBlank() },
      detail = m.optString("detail", "").takeIf { it.isNotBlank() },
      imageB64 = m.optString("image_b64", "").takeIf { it.isNotBlank() }
    )
  }

  fun ack(messageId: Long, action: String, source: String) {
    val connection = open("/ack", "POST")
    val payload = JSONObject()
      .put("message_id", messageId)
      .put("action", action)
      .put("source", source)
      .toString()
    connection.outputStream.use { it.write(payload.toByteArray(Charsets.UTF_8)) }
    readResponse(connection)
  }

  fun health(): Boolean {
    val connection = URL(baseUrl.trimEnd('/') + "/health").openConnection() as HttpURLConnection
    connection.connectTimeout = 3000
    connection.readTimeout = 3000
    return connection.responseCode == 200
  }

  private fun open(path: String, method: String): HttpURLConnection {
    val connection = URL(baseUrl.trimEnd('/') + path).openConnection() as HttpURLConnection
    connection.requestMethod = method
    connection.connectTimeout = 3000
    connection.readTimeout = 5000
    connection.setRequestProperty("X-Bus-Token", token)
    if (method == "POST") {
      connection.doOutput = true
      connection.setRequestProperty("Content-Type", "application/json")
    }
    return connection
  }

  private fun readResponse(connection: HttpURLConnection): String {
    val stream = if (connection.responseCode in 200..299) connection.inputStream else connection.errorStream
    val text = stream?.bufferedReader(Charsets.UTF_8)?.readText().orEmpty()
    check(connection.responseCode in 200..299) { "HTTP ${connection.responseCode}: $text" }
    return text
  }
}
