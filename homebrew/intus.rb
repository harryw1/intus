class Intus < Formula
  desc "Local Autonomous Agent and System Sidecar for your terminal"
  homepage "https://github.com/harryw1/intus"
  url "https://github.com/harryw1/intus/archive/refs/tags/v1.0.3.tar.gz"
  sha256 "c27bd888e6afa5cad254db7cd85ed41b6a54d3362073c27cb678a681042e24ec"
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
