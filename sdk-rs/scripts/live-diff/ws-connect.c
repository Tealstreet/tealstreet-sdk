// Probe-only terminal socket redirection. Loaded only into keyless child
// processes; production Rust/TS constructors and WebSocket codecs stay intact.
// HTTPS REST already uses the loopback proxy, so only direct TLS port 443
// sockets reach this hook. TLS hostname validation still uses the original SNI.
#include <arpa/inet.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

static int redirected_connect(int fd, const struct sockaddr *address, socklen_t length) {
  struct sockaddr_storage copy;
  const char *port = NULL;
  if (address && length <= sizeof(copy)) {
    memcpy(&copy, address, length);
    if (address->sa_family == AF_INET && length >= sizeof(struct sockaddr_in)) {
      struct sockaddr_in *target = (struct sockaddr_in *)&copy;
      if (ntohs(target->sin_port) == 443 && (port = getenv("SAFE_CEX_PROBE_TLS_PORT"))) {
        target->sin_port = htons((unsigned short)atoi(port));
        target->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        address = (const struct sockaddr *)target;
      }
    } else if (address->sa_family == AF_INET6 && length >= sizeof(struct sockaddr_in6)) {
      struct sockaddr_in6 *target = (struct sockaddr_in6 *)&copy;
      if (ntohs(target->sin6_port) == 443 && (port = getenv("SAFE_CEX_PROBE_TLS_PORT6"))) {
        target->sin6_port = htons((unsigned short)atoi(port));
        target->sin6_addr = in6addr_loopback;
        target->sin6_scope_id = 0;
        address = (const struct sockaddr *)target;
      }
    }
  }
#ifdef __APPLE__
  return connect(fd, address, length);
#else
  static int (*original)(int, const struct sockaddr *, socklen_t);
  if (!original) original = dlsym(RTLD_NEXT, "connect");
  return original(fd, address, length);
#endif
}

#ifdef __APPLE__
__attribute__((used, section("__DATA,__interpose")))
static struct { const void *replacement; const void *original; } interpose_connect = {
  (const void *)redirected_connect, (const void *)connect
};
#else
int connect(int fd, const struct sockaddr *address, socklen_t length) {
  return redirected_connect(fd, address, length);
}
#endif
