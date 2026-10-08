import { OpenCode } from "@opencode/client"
import { Service } from "@opencode/client/service"

// V2.0.16 exposes message lists and forms on the HTTP client, but not on Plugin.Context.
// An explicit `serve` process must call its own listener, not another managed service.
export async function serviceClient(options: Record<string, unknown> = {}) {
  let url = typeof options.serverURL === "string" ? options.serverURL : undefined
  const args = process.argv
  if (!url && args.includes("serve") && !args.includes("--service")) {
    const portIndex = args.indexOf("--port")
    const port = Number(portIndex >= 0 ? args[portIndex + 1] : args.find((arg) => arg.startsWith("--port="))?.slice(7))
    if (!Number.isInteger(port) || port <= 0) throw new Error("Set the plugin serverURL option for a server without an explicit listening port")
    const hostIndex = args.indexOf("--hostname")
    const hostname = (hostIndex >= 0 ? args[hostIndex + 1] : args.find((arg) => arg.startsWith("--hostname="))?.slice(11)) ?? "127.0.0.1"
    const host = hostname === "0.0.0.0" || hostname === "::" ? "127.0.0.1" : hostname.includes(":") ? `[${hostname}]` : hostname
    url = `http://${host}:${port}`
  }
  if (url) {
    const password = process.env.OPENCODE_SERVER_PASSWORD
    const username = process.env.OPENCODE_SERVER_USERNAME ?? "opencode"
    return OpenCode.make({ baseUrl: url, headers: password ? { authorization: `Basic ${Buffer.from(`${username}:${password}`).toString("base64")}` } : undefined })
  }
  const endpoint = await Service.discover()
  if (!endpoint) throw new Error("OpenCode background service is unavailable")
  return OpenCode.make({ baseUrl: endpoint.url, headers: Service.headers(endpoint) })
}
