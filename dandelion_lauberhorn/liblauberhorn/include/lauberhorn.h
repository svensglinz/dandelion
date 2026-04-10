/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2025 Pengcheng Xu */

#ifndef LAUBERHORN_H
#define LAUBERHORN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "oncrpc.h"

typedef struct lauberhorn {
  /* to /dev/lauberhorn */
  int fd;
  uint8_t *parity_page;
} lauberhorn_t;

typedef struct lauberhorn_oncrpc_schema lauberhorn_schema_t;

// Unmarshalled XDR data.  To be marshalled/unmarshalled according to
// the lauberhorn_schema_t on initialization
typedef void *lauberhorn_msg_t;

// (app data, unmarshalled request, xid) -> unmarshalled response
typedef lauberhorn_msg_t (*lauberhorn_handler_t)(void *, lauberhorn_msg_t, int);

// Register application

int lauberhorn_init(lauberhorn_t *ctx);
void lauberhorn_fini(lauberhorn_t *ctx);

/**
 * Register an RPC service with lauberhorn
 * Returns: index of registered service on success, -1 on failure
 */
int lauberhorn_reg_srv(lauberhorn_t *ctx, lauberhorn_handler_t func, void *data,
                       int prog_num, int prog_ver, int proc_num,
                       uint16_t listen_port, lauberhorn_schema_t *schema);

/**
 * deregister an RPC service with the lauberhorn NIC
 * 
 * removes the service from the dispatch table
 * and unregisters it from the portmapper. 
 * 
 * @ctx: lauebrhorn context. Must have been initialized
 * via lauberhorn_init() 
 * @srv_id: service id to be deregistered
 * 
 * Returns 0 on success, - 1 on error
 */
int lauberhorn_dereg_srv(lauberhorn_t *ctx, int srv_id);



struct lauberhorn_worker;
typedef struct lauberhorn_worker *lauberhorn_worker_t;

// Per-worker init and clean-up callback functions from the user
typedef void (*lauberhorn_user_cb_t)(int);
__attribute__((unused)) static void noop_cb(int) {}


/**
 * Spawn a worker thread that services incoming ONC RPC calls via the ECI
 * shared-memory channel.
 *
 * @ctx:  Runtime context returned by lauberhorn_init().  Must remain valid
 *        for the lifetime of the worker.
 * @init: Called once on thread start before entering the dispatch loop.
 *        Receives the worker index. May be NULL.
 * @fini: Called once on thread exit after the dispatch loop returns.
 *        Receives the same worker index. May be NULL.
 *
 * Returns: Opaque worker handle on success, or LAUBERHORN_WORKER_INVALID on
 *          failure (check errno). Pass to lauberhorn_join_worker() for clean
 *          shutdown.
 */
lauberhorn_worker_t lauberhorn_create_worker(lauberhorn_t *ctx,
                                             lauberhorn_user_cb_t init,
                                             lauberhorn_user_cb_t fini);
/**
 * 
 */
void lauberhorn_join_worker(lauberhorn_t *ctx, lauberhorn_worker_t w);

#endif // LAUBERHORN_H