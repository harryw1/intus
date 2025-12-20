class Intus < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/intus"
  url "https://github.com/harryw1/intus/archive/refs/tags/v1.0.2.tar.gz"
  sha256 "2824fea4b5bf2c0dae96e08f867579abe59e42030b0989c51bce748ec806bb7f" # Update this after tagging
  license "MIT"
  head "https://github.com/harryw1/intus.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "intus", shell_output("#{bin}/intus --version")
  end
end
