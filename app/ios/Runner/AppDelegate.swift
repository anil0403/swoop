import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    Swoop.register(with: engineBridge.pluginRegistry.registrar(forPlugin: "Swoop")!)
  }
}

/// The app's side of the "uft" channel, as MainActivity.kt is on Android: the device's name and
/// save folder, swoop:// links opened (a QR code scanned with the camera), the Files app, and the
/// screen kept on while something transfers, since a locked iPhone suspends the app.
class Swoop: NSObject, FlutterPlugin, FlutterSceneLifeCycleDelegate {
  private var channel: FlutterMethodChannel?
  private var link: String?

  static func register(with registrar: FlutterPluginRegistrar) {
    let s = Swoop()
    let channel = FlutterMethodChannel(name: "uft", binaryMessenger: registrar.messenger())
    s.channel = channel
    registrar.addMethodCallDelegate(s, channel: channel)
    registrar.addSceneDelegate(s)
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "info":
      // Documents shows in the Files app, as On My iPhone > Swoop.
      let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      result(["name": UIDevice.current.name, "downloads": docs.path])
    case "openedLink":
      result(link)
      link = nil
    case "shared":
      result([])
    case "busy":
      UIApplication.shared.isIdleTimerDisabled = call.arguments as? Bool ?? false
      result(nil)
    case "openDir":
      let path = (call.arguments as? String ?? "")
        .addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? ""
      guard let url = URL(string: "shareddocuments://" + path) else {
        result(FlutterError(code: "openDir", message: "bad path", details: nil))
        return
      }
      UIApplication.shared.open(url) { ok in
        result(ok ? nil : FlutterError(code: "openDir", message: "the Files app didn't open it", details: nil))
      }
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  // The selectors spelled out: these are optional, so a misspelled one would never be called.
  @objc(scene:willConnectToSession:options:)
  func scene(
    _ scene: UIScene, willConnectTo session: UISceneSession,
    options connectionOptions: UIScene.ConnectionOptions?
  ) -> Bool {
    guard let url = connectionOptions?.urlContexts.first?.url, url.scheme == "swoop" else { return false }
    opened(url)
    return true
  }

  @objc(scene:openURLContexts:)
  func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) -> Bool {
    guard let url = URLContexts.first?.url, url.scheme == "swoop" else { return false }
    opened(url)
    return true
  }

  private func opened(_ url: URL) {
    link = url.absoluteString
    channel?.invokeMethod("intent", arguments: nil)
  }
}
