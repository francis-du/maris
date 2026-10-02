// Actual WebKit visual review of the generated local site. No network or audio activation.
import AppKit
import Foundation
import WebKit

final class Review: NSObject, WKNavigationDelegate {
    let root: URL
    let output: URL
    let page: String
    let window: NSWindow
    let web: WKWebView
    var done = false
    var failure: String?
    init(root: URL, page: String, width: Int, output: URL) {
        self.root = root; self.page = page; self.output = output
        let frame = NSRect(x: 0, y: 0, width: width, height: width < 700 ? 1000 : 1100)
        window = NSWindow(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: false)
        web = WKWebView(frame: frame)
        super.init()
        window.setFrameOrigin(NSPoint(x: -10000, y: -10000))
        window.contentView = web
        web.navigationDelegate = self
    }
    func start() {
        window.orderFront(nil)
        web.loadFileURL(root.appendingPathComponent(page), allowingReadAccessTo: root)
    }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // Deliberately request below-fold lazy images for the test without changing production HTML.
        web.evaluateJavaScript("for (const image of document.images) { image.loading = 'eager'; }") { _, error in
            if let error = error { self.failure = error.localizedDescription; self.done = true; return }
            self.captureAfterLoad()
        }
    }
    func captureAfterLoad() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            self.web.evaluateJavaScript("JSON.stringify({width:innerWidth,scrollWidth:document.documentElement.scrollWidth,lang:document.documentElement.lang,images:[...document.images].every(i=>i.complete&&i.naturalWidth>0),imageStatus:[...document.images].map(i=>({src:i.getAttribute('src'),complete:i.complete,width:i.naturalWidth,loading:i.loading,top:Math.round(i.getBoundingClientRect().top)})),title:document.title})") { value, error in
                if let error = error { self.failure = error.localizedDescription; self.done = true; return }
                guard let text = value as? String,
                      let data = text.data(using: .utf8),
                      let info = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      let width = info["width"] as? Int, let scroll = info["scrollWidth"] as? Int,
                      scroll <= width, info["images"] as? Bool == true else {
                    self.failure = "Page overflow or missing local images: \(value ?? "nil")"; self.done = true; return
                }
                print("Browser check: \(self.page) \(text)")
                let config = WKSnapshotConfiguration()
                config.rect = self.web.bounds
                self.web.takeSnapshot(with: config) { image, error in
                    defer { self.done = true }
                    guard error == nil, let image = image, let tiff = image.tiffRepresentation,
                          let bitmap = NSBitmapImageRep(data: tiff),
                          let png = bitmap.representation(using: .png, properties: [:]) else {
                        self.failure = "WebKit snapshot failed"; return
                    }
                    do { try png.write(to: self.output, options: .atomic); print("Rendered \(self.output.lastPathComponent)") }
                    catch { self.failure = error.localizedDescription }
                }
            }
        }
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        failure = error.localizedDescription; done = true
    }
}
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent("_site")
let output = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent(".maris-review")
let cases: [(String, Int, String)] = [
    ("zh-CN/index.html", 1440, "site-zh-home-wide.png"),
    ("en/index.html", 1440, "site-en-home-wide.png"),
    ("zh-CN/install.html", 390, "site-zh-install-small.png"),
    ("de/guide.html", 1000, "site-de-guide.png")
]
for (page, width, name) in cases {
    let review = Review(root: root, page: page, width: width, output: output.appendingPathComponent(name))
    review.start()
    let deadline = Date().addingTimeInterval(25)
    while !review.done && Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
    review.window.orderOut(nil)
    if !review.done || review.failure != nil {
        fputs("Site browser review failed: \(review.failure ?? "timed out")\n", stderr)
        exit(1)
    }
}
print("Local browser review completed; no deployment or hardware validation claimed.")
