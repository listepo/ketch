// Checks the appcast a macOS app release is about to publish against the app
// it ships, the way Sparkle inside that app will:
//
//   xcrun swift scripts/desktop-appcast-verify.swift <Ketch.app> <appcast.xml> <Ketch-X.Y.Z.dmg> <X.Y.Z>
//
// generate_appcast signs with whatever SPARKLE_ED_PRIVATE_KEY holds, and the
// app trusts only the SUPublicEDKey it was built with. If those two are not a
// pair, every installed copy rejects the update, and nothing fails until a
// user clicks "Install". So the release verifies the enclosure's EdDSA
// signature with the public key read out of the exported app, and stops
// before tagging when it does not hold. generate_appcast itself only prints a
// warning and writes the item unsigned when the keys differ.

import CryptoKit
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("desktop-appcast-verify: \(message)\n".utf8))
    exit(1)
}

let args = CommandLine.arguments
guard args.count == 5 else {
    fail("usage: desktop-appcast-verify.swift <Ketch.app> <appcast.xml> <Ketch-X.Y.Z.dmg> <X.Y.Z>")
}
let app = URL(fileURLWithPath: args[1])
let appcast = URL(fileURLWithPath: args[2])
let dmg = URL(fileURLWithPath: args[3])
let version = args[4]

guard let info = Bundle(url: app)?.infoDictionary else { fail("no Info.plist in \(app.path)") }
guard let keyText = info["SUPublicEDKey"] as? String,
    let keyData = Data(base64Encoded: keyText),
    let publicKey = try? Curve25519.Signing.PublicKey(rawRepresentation: keyData)
else { fail("SUPublicEDKey in \(app.path) is not a base64 Ed25519 public key") }
guard info["CFBundleVersion"] as? String == version else {
    fail("CFBundleVersion is \(info["CFBundleVersion"] ?? "missing"), expected \(version)")
}

guard let feedText = try? String(contentsOf: appcast, encoding: .utf8) else { fail("cannot read \(appcast.path)") }
// SURequireSignedFeed is on in the app, so an unsigned feed is refused whole.
guard feedText.contains("sparkle-signatures:") else { fail("the appcast carries no feed signature") }
guard let feed = try? XMLDocument(data: Data(feedText.utf8)) else { fail("the appcast is not XML") }

let items = (try? feed.nodes(forXPath: "//item")) ?? []
let name = dmg.lastPathComponent
guard
    let item = items.compactMap({ $0 as? XMLElement }).first(where: { item in
        let url = item.elements(forName: "enclosure").first?.attribute(forName: "url")?.stringValue
        return url.flatMap(URL.init(string:))?.lastPathComponent == name
    })
else { fail("no item in the appcast points at \(name)") }
guard let enclosure = item.elements(forName: "enclosure").first else { fail("\(name) has no enclosure") }

let itemVersion =
    item.elements(forName: "sparkle:version").first?.stringValue
    ?? enclosure.attribute(forName: "sparkle:version")?.stringValue
guard itemVersion == version else { fail("the item for \(name) says version \(itemVersion ?? "nothing")") }

guard let signatureText = enclosure.attribute(forName: "sparkle:edSignature")?.stringValue,
    let signature = Data(base64Encoded: signatureText)
else {
    // generate_appcast only warns, and leaves the signature out, when its key
    // does not match the app's SUPublicEDKey.
    fail("the enclosure for \(name) has no sparkle:edSignature: SPARKLE_ED_PRIVATE_KEY is not the app's key pair")
}
guard let archive = try? Data(contentsOf: dmg) else { fail("cannot read \(dmg.path)") }
guard enclosure.attribute(forName: "length")?.stringValue == String(archive.count) else {
    fail("the enclosure length does not match \(name)'s \(archive.count) bytes")
}
guard publicKey.isValidSignature(signature, for: archive) else {
    fail(
        "the signature on \(name) does not verify with the app's SUPublicEDKey: SPARKLE_ED_PRIVATE_KEY is not its pair")
}
print("desktop-appcast-verify: \(name) \(version) verifies with the app's key")
