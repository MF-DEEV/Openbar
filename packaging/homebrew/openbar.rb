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

  # Every Mac release so far is a GitHub pre-release, which the default
  # strategy skips; Mac tags are v<version>, Windows and Linux tags differ.
  livecheck do
    url "https://github.com/MF-DEEV/Openbar"
    regex(/^v(\d+(?:\.\d+)+(?:-[\w.]+)?)$/i)
    strategy :github_releases do |json, regex|
      json.map do |release|
        next if release["draft"]

        match = release["tag_name"]&.match(regex)
        next if match.blank?

        match[1]
      end
    end
  end

  depends_on macos: :sonoma

  app "Openbar.app"

  # Openbar isn't notarized yet, so Gatekeeper would refuse to open it after
  # the download. The release DMG is the one GitHub serves for this tag, and
  # Homebrew checked its SHA-256 above.
  postflight_steps do
    run "/usr/bin/xattr",
        args:           ["-dr", "com.apple.quarantine", "{{appdir}}/Openbar.app"],
        writable_paths: ["Openbar.app"],
        writable_base:  :appdir
  end

  uninstall quit: "com.openbar.app"

  zap trash: [
    "~/Library/Application Support/com.openbar.app",
    "~/Library/Caches/com.openbar.app",
    "~/Library/HTTPStorages/com.openbar.app",
    "~/Library/Preferences/com.openbar.app.plist",
  ]
end
