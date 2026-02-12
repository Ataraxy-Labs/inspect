import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  basePath: "/inspect",
  output: "export",
  eslint: {
    ignoreDuringBuilds: true,
  },
};

export default nextConfig;
