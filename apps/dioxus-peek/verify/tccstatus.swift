import AVFoundation
let names = [0: "notDetermined", 1: "restricted", 2: "denied", 3: "authorized"]
let cam = AVCaptureDevice.authorizationStatus(for: .video).rawValue
let mic = AVCaptureDevice.authorizationStatus(for: .audio).rawValue
print("camera=\(names[cam] ?? "?") mic=\(names[mic] ?? "?")")
