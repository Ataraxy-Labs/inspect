import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  basePath: "/inspect",
  eslint: {
    ignoreDuringBuilds: true,
  },
};

export default nextConfig;
