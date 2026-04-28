import AppKit
import Foundation

enum HelperError: Error {
    case usage(String)
    case clipboard(String)
}

func mimeToPasteboardType(_ mime: String) -> NSPasteboard.PasteboardType {
    switch mime {
    case "text/plain":
        return .string
    case "text/html":
        return NSPasteboard.PasteboardType("public.html")
    case "image/png":
        return .png
    case "text/uri-list":
        return NSPasteboard.PasteboardType("public.file-url")
    default:
        return NSPasteboard.PasteboardType(mime)
    }
}

func readStdin() -> Data {
    FileHandle.standardInput.readDataToEndOfFile()
}

func writeStdout(_ data: Data) {
    FileHandle.standardOutput.write(data)
}

func readUriList() throws -> Data {
    guard let values = pasteboard.readObjects(forClasses: [NSURL.self], options: nil) as? [NSURL],
        !values.isEmpty
    else {
        throw HelperError.clipboard("text/uri-list is not available in the clipboard")
    }

    let text = values
        .map { ($0 as URL).absoluteString }
        .joined(separator: "\n")
    return Data(text.utf8)
}

func parseUriList(_ payload: Data) throws -> [NSURL] {
    guard let text = String(data: payload, encoding: .utf8) else {
        throw HelperError.clipboard("text/uri-list payload must be valid UTF-8")
    }

    let urls = try text
        .split(whereSeparator: \Character.isNewline)
        .map(String.init)
        .filter { !$0.isEmpty && !$0.hasPrefix("#") }
        .map { value throws -> NSURL in
            guard let url = URL(string: value) else {
                throw HelperError.clipboard("text/uri-list contained an invalid URL")
            }
            return url as NSURL
        }

    guard !urls.isEmpty else {
        throw HelperError.clipboard("text/uri-list payload did not contain any URLs")
    }

    return urls
}

func parseUtf8Text(_ payload: Data, mime: String) throws -> String {
    guard let text = String(data: payload, encoding: .utf8) else {
        throw HelperError.clipboard("\(mime) payload must be valid UTF-8")
    }
    return text
}

func makePasteboardItem(mime: String, payload: Data) throws -> NSPasteboardItem {
    let item = NSPasteboardItem()
    let type = mimeToPasteboardType(mime)

    if mime == "text/plain" {
        let text = try parseUtf8Text(payload, mime: mime)
        guard item.setString(text, forType: .string) else {
            throw HelperError.clipboard("failed to prepare text/plain")
        }
        return item
    }

    if mime == "text/html" {
        let text = try parseUtf8Text(payload, mime: mime)
        guard item.setData(payload, forType: type) else {
            throw HelperError.clipboard("failed to prepare \(mime)")
        }
        guard item.setString(text, forType: .string) else {
            throw HelperError.clipboard("failed to prepare text/plain fallback for \(mime)")
        }
        return item
    }

    guard item.setData(payload, forType: type) else {
        throw HelperError.clipboard("failed to prepare \(mime)")
    }
    return item
}

func requireType(_ args: [String]) throws -> String {
    guard let index = args.firstIndex(of: "--type"), args.indices.contains(index + 1) else {
        throw HelperError.usage("missing --type")
    }
    return args[index + 1]
}

let args = Array(CommandLine.arguments.dropFirst())
let pasteboard = NSPasteboard.general

do {
    guard let command = args.first else {
        throw HelperError.usage("expected one of: types, read, write")
    }

    switch command {
    case "types":
        let values = (pasteboard.types ?? []).map(\.rawValue).joined(separator: "\n")
        writeStdout(Data(values.utf8))
    case "read":
        let mime = try requireType(args)
        let type = mimeToPasteboardType(mime)
        if mime == "text/plain" {
            guard let text = pasteboard.string(forType: .string) else {
                throw HelperError.clipboard("text/plain is not available in the clipboard")
            }
            writeStdout(Data(text.utf8))
        } else if mime == "text/uri-list" {
            writeStdout(try readUriList())
        } else {
            guard let data = pasteboard.data(forType: type) else {
                throw HelperError.clipboard("\(mime) is not available in the clipboard")
            }
            writeStdout(data)
        }
    case "write":
        let mime = try requireType(args)
        let payload = readStdin()
        if mime == "text/uri-list" {
            let text = try parseUtf8Text(payload, mime: mime)
            let urls = try parseUriList(payload)
            pasteboard.clearContents()
            guard pasteboard.writeObjects(urls) else {
                throw HelperError.clipboard("failed to write \(mime)")
            }
            guard pasteboard.setString(text, forType: .string) else {
                throw HelperError.clipboard("failed to write text/plain fallback for \(mime)")
            }
        } else {
            let item = try makePasteboardItem(mime: mime, payload: payload)
            pasteboard.clearContents()
            guard pasteboard.writeObjects([item]) else {
                throw HelperError.clipboard("failed to write \(mime)")
            }
        }
    default:
        throw HelperError.usage("expected one of: types, read, write")
    }
} catch let error as HelperError {
    switch error {
    case .usage(let message), .clipboard(let message):
        fputs("\(message)\n", stderr)
        exit(1)
    }
} catch {
    fputs("\(error)\n", stderr)
    exit(1)
}
