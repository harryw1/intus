class Tenere < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/fiesty/tenere"
  url "https://github.com/fiesty/tenere/archive/refs/tags/v1.0.0.tar.gz" # Update this after tagging
  sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Update this after tagging
  license "MIT"
  head "https://github.com/fiesty/tenere.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "tenere", shell_output("#{bin}/tenere --version")
  end
end
