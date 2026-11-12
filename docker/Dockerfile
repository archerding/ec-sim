# EC-Sim 无特权容器镜像（M1 退出项：Docker 无特权跑通）
#
# 构建（需可访问容器 registry 拉取 Rust/Debian 基础镜像）：
#   docker build -f docker/Dockerfile -t ecsim-uds:latest .
# 运行（**不需要 --privileged / --network=host**）：
#   docker run --rm -v /tmp/ecsim-sock:/sock ecsim-uds:latest --uds /sock/ecsim.sock --slaves 8
# 然后在宿主机用 `scripts/spikes/uds_master_probe.py /tmp/ecsim-sock/ecsim.sock --multi-slave=8` 验证。
#
# 说明：UDS shim 只用 Unix domain socket，不需要 TAP/AF_PACKET/root；容器以默认用户运行。
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p esc-node

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/ec-sim-esc /usr/local/bin/ec-sim-esc
ENTRYPOINT ["ec-sim-esc"]
CMD ["--uds", "/tmp/ecsim.sock"]
