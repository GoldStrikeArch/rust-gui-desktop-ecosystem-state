import AVFoundation
guard let d = AVCaptureDevice.default(for: .video) else { print("no camera"); exit(1) }
print("device:", d.localizedName, "inUseByAnotherApplication:", d.isInUseByAnotherApplication)
do { try d.lockForConfiguration(); print("lockForConfiguration: OK"); d.unlockForConfiguration() }
catch { print("lockForConfiguration FAILED:", error.localizedDescription) }
