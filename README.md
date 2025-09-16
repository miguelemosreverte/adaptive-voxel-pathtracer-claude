# Adaptive Voxel Path Tracer

*Real-time WebGPU path tracer with performance-driven adaptive level-of-detail*

## 🎯 Project Overview

This project implements a revolutionary approach to real-time path tracing using adaptive voxel resolution. Unlike traditional renderers, it automatically scales scene complexity based on hardware performance, ensuring consistent framerates from weak CPUs to high-end GPUs.

## 📋 Quick Start

1. **Read the full specification**: See [`CLAUDE.md`](./CLAUDE.md) for complete technical details
2. **Clone reference repository**: `git clone https://github.com/Zydak/Vulkan-Path-Tracer.git`
3. **Install Rust**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
4. **Create project**: `cargo new adaptive-voxel-pathtracer --bin`

## 🎨 Key Innovation

**Performance-Adaptive Voxelization**: Scene detail automatically adjusts from simple cubes on weak hardware to photorealistic detail on powerful systems, maintaining 20+ FPS target.

## 📚 Documentation

- **[CLAUDE.md](./CLAUDE.md)**: Complete technical specification
- **Reference**: [Zydak's Vulkan Path Tracer](https://github.com/Zydak/Vulkan-Path-Tracer)
- **Target Platform**: WebGPU + Rust (wgpu crate)
- **Performance Goal**: 20+ FPS on Apple M1 MacBook

## 🚀 Implementation Status

- ✅ **Architecture Designed**: Complete system specification
- ⏳ **Phase 1**: Foundation setup (WebGPU + basic voxel rendering)
- ⏳ **Phase 2**: Octree implementation with LoD
- ⏳ **Phase 3**: Adaptive performance system
- ⏳ **Phase 4**: Dynamic scene updates
- ⏳ **Phase 5**: Advanced materials & optimization

---

*Created: September 16, 2025 | Inspired by Zydak's Vulkan Path Tracer*