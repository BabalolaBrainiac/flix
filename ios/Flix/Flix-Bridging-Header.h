#ifndef Flix_Bridging_Header_h
#define Flix_Bridging_Header_h

#include <stdbool.h>
#include <stdint.h>

typedef int32_t (*FlixLaunchCallback)(const char *payload_json);
typedef int32_t (*FlixStopCallback)(void);

void flix_ios_register_player_callbacks(FlixLaunchCallback on_launch, FlixStopCallback on_stop);
void flix_ios_clear_player_callbacks(void);
char *flix_ios_start(uint16_t port);
void flix_ios_stop(void);
void flix_ios_report_player_exit(bool success, int32_t exit_code);
void flix_ios_free_string(char *ptr);

#endif
