# SPDX-License-Identifier: GPL-3.0-or-later
# Openbar's cask in the MF-DEEV/homebrew-openbar tap. Tools/bump-packages.sh
# fills in the version and checksum and pushes it to the tap on each release.

cask "openbar" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/MF-DEEV/Openbar/releases/download/v#{version}/Openbar-#{version}.dmg"
  name "Openbar"
  desc "Dynamic Island for the notch with your AI coding agents' sessions"
  homepage "https://github.com/MF-DEEV/Openbar"

  livecheck do
    url :url
    regex(/^v?(\d+(?:\.\d+)+(?:-[\w.]+)?)$/i)
    strategy :github_releases
  end

  depends_on macos: ">= :sonoma"

  app "Openbar.app"

  # Openbar isn't notarized yet, so Gatekeeper would refuse to open it after
  # the download. The release DMG is the one GitHub serves for this tag, and
  # Homebrew checked its SHA-256 above.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Openbar.app"]
  end

  uninstall quit: "com.openbar.app"

  zap trash: [
    "~/Library/Application Support/com.openbar.app",
    "~/Library/Caches/com.openbar.app",
    "~/Library/HTTPStorages/com.openbar.app",
    "~/Library/Preferences/com.openbar.app.plist",
  ]
end
