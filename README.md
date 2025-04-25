# 🌠 GPU Game of Life

A high-performance cellular automaton simulator powered by GPU compute shaders. This implementation of Conway's Game of Life runs entirely on your graphics card for blazing-fast simulation of large grids.

![Game of Life](https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExcjQxM2s4cTRvM3BwdXQxZW41Y3NqcmIzMHVlaGlscnJtbGUyMHJvbCZlcD12MV9pbnRlcm5hbF9naWZfYnlfaWQmY3Q9Zw/3og0ILr4ttBPRBjYoE/giphy.gif)

## ✨ Features

- **GPU-Accelerated**: Utilizes WGPU compute shaders for massively parallel simulation
- **Interactive**: Paint living cells with your mouse
- **Zoom & Pan**: Navigate across the simulation space
- **Lucky Cells**: 10% chance for dying cells to survive and turn red!
- **Configurable Rules**: Classic Conway's rules with customization options

## 🎮 Controls

- **Left Mouse Button**: Draw living cells
- **Right Mouse Button + Drag**: Pan around the grid
- **Mouse Wheel**: Zoom in/out
- **Escape**: Exit

## 🧬 Simulation Rules

This simulation follows Conway's Game of Life rules with an extra twist:

1. Any live cell with fewer than 2 live neighbors dies (underpopulation)
2. Any live cell with 2 or 3 live neighbors survives
3. Any live cell with more than 3 live neighbors dies (overpopulation)
4. Any dead cell with exactly 3 live neighbors becomes alive
5. **Special Rule**: When a cell would normally die, it has a 10% chance to survive and turn red!

## 🔧 Technical Details

Built with Rust and WGPU, this simulator leverages your GPU's parallel processing power:

- **Rust**: Safe, concurrent, and high-performance language
- **WGPU**: Cross-platform GPU compute and rendering
- **Compute Shaders**: WGSL code running directly on the GPU
- **Double Buffering**: Ping-pong buffer technique for cellular simulation

The simulation state is represented as:
- `0.0`: Dead cell (black)
- `1.0`: Live cell (white)
- `2.0`: Lucky cell that survived death (red)

## 🚀 Getting Started

### Requirements
- Rust 1.60+ and Cargo
- A GPU with support for compute shaders

### Building and Running

```bash
# Clone the repository
git clone https://github.com/yourusername/gpu-game-of-life.git
cd gpu-game-of-life

# Build and run in release mode (recommended for performance)
cargo run --release
```

## 🎯 Performance

The GPU implementation allows for real-time simulation of much larger grids than CPU-based approaches:

- Easily handles 1024×1024 grids at 60+ FPS on most modern GPUs
- Brush tool with adjustable size for easy pattern creation
- Efficient zoom and pan implementation for navigating large simulations

## 🛠️ Future Improvements

- More patterns and presets
- Additional cellular automaton rule sets
- Statistics display (population, generation count)
- Simulation speed control
- Save/load functionality

## 📜 License

This project is licensed under the MIT License - see the LICENSE file for details.

---

Made with ❤️ using Rust and WGPU 