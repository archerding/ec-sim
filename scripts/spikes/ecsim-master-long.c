/* task-78：单次连续 SOEM 主站会话（长跑），保持 8 站稳定 OP，不重启 master。
 * 构建：bash scripts/spikes/build-ecsim-master-long.sh（VM 上，链接 fixtures 的 libsoem.a）
 * 运行：/root/ecsim-master-long <ifname> [hours]
 * 输出：每秒打印 AL 状态/帧计数到 stdout（由 c4 脚本重定向），退出码 0=正常到时。
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include "soem/soem.h"

static char IOmap[4096];
static int expected_wkc = 0;

static int slave_setup(uint16_t slave) {
  int size = sizeof(IOmap);
  return ecx_config_map_group(&ecx_context, IOmap, slave);
}

int main(int argc, char **argv) {
  const char *ifname = argc > 1 ? argv[1] : "escsim0";
  const double hours = argc > 2 ? atof(argv[2]) : 4.0;
  if (!ecx_init(&ecx_context, ifname)) {
    fprintf(stderr, "MASTER_LONG init failed if=%s\n", ifname);
    return 2;
  }
  printf("MASTER_LONG if=%s state=init\n", ifname);
  fflush(stdout);
  if (ecx_config_init(&ecx_context) <= 0) {
    fprintf(stderr, "MASTER_LONG config_init: no slaves\n");
    ecx_close(&ecx_context);
    return 3;
  }
  printf("MASTER_LONG slaves=%d\n", ecx_slavecount);
  fflush(stdout);
  ecx_config_map_group(&ecx_context, IOmap, 0);
  ecx_configdc(&ecx_context);
  for (int i = 1; i <= ecx_slavecount; i += 1) {
    expected_wkc += ecx_slave[i].outputsWKC * 2 + ecx_slave[i].inputsWKC;
  }
  printf("MASTER_LONG expected_wkc=%d\n", expected_wkc);
  ecx_statecheck(&ecx_context, 0, EC_STATE_SAFE_OP, EC_TIMEOUTSTATE * 4);
  ecx_slave[0].state = EC_STATE_OPERATIONAL;
  ecx_send_processdata(&ecx_context);
  ecx_writestate(&ecx_context, 0);
  time_t started = time(NULL);
  long cycles = 0;
  int reported_op = 0;
  while ((time(NULL) - started) < (time_t)(hours * 3600)) {
    ecx_send_processdata(&ecx_context);
    int wkc = ecx_receive_processdata(&ecx_context, EC_TIMEOUTRET);
    cycles += 1;
    if (cycles % 1000 == 0) {
      int al = ecx_slave[0].state;
      int op_count = 0;
      for (int i = 1; i <= ecx_slavecount; i += 1) {
        if (ecx_slave[i].state == EC_STATE_OPERATIONAL) op_count += 1;
      }
      printf("MASTER_LONG cycle=%ld wkc=%d expected=%d op=%d/%d al=0x%02x uptime=%ld\n",
             cycles, wkc, expected_wkc, op_count, ecx_slavecount, al, (long)(time(NULL) - started));
      fflush(stdout);
      if (op_count == ecx_slavecount) reported_op = 1;
      if (wkc < 0) usleep(1000);
    }
    usleep(1000); /* ~1 kHz 循环，避免空转 */
  }
  ecx_slave[0].state = EC_STATE_INIT;
  ecx_writestate(&ecx_context, 0);
  ecx_close(&ecx_context);
  printf("MASTER_LONG normal_exit cycles=%ld op_seen=%d\n", cycles, reported_op);
  return 0;
}
