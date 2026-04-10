/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2025 Pengcheng Xu */

#ifndef LAUBERHORN_RT_ONCRPC_H
#define LAUBERHORN_RT_ONCRPC_H

#include <rpc/rpc.h>
#include <rpc/xdr.h>

// Runtime-facing functions to marshal and unmarshal a message
// according to a schema

// Allocate a request message for the runtime.  The user handler
// owns the response message so we don't allocate them here

// User-facing functions to create and manipulate an ONC-RPC schema

typedef void* lauberhorn_msg_t; 

struct lauberhorn_oncrpc_schema {
  xdrproc_t call_func;
  xdrproc_t resp_func;

  /* size of unmarshalled data stream of caller */
  size_t call_size;
};


lauberhorn_msg_t lauberhorn_oncrpc_req_alloc(struct lauberhorn_oncrpc_schema *schema);

void lauberhorn_oncrpc_req_free(struct lauberhorn_oncrpc_schema *schema,
                                lauberhorn_msg_t msg);

/**
 * Marshal data stream in in_msg according to the XDR procedure pointed to
 * by schema->resp_func and write to out_buf.
 *
 * @schema: ONC RPC schema containing the XDR encode procedure
 * @out_buf: buffer marshalled stream is written to
 * @out_buf_size: size of out_buf in bytes
 * @in_msg: message to be marshalled
 * @return: TRUE on success, FALSE if XDR encoding failed (e.g. buffer too
 * small)
 */
int lauberhorn_oncrpc_marshal(struct lauberhorn_oncrpc_schema *schema,
                              uint8_t *out_buf, size_t out_buf_size,
                              lauberhorn_msg_t in_msg);

/**
 * Unmarshal data stream in in_buf according to the XDR procedure pointed to
 * by schema->call_func and write to out_msg
 *
 * @schema:    ONC RPC schema containing the XDR decode procedure and call
 * struct size
 * @out_msg:   destination buffer for decoded struct, must be at least
 * schema->call_size bytes, typically allocated via
 * lauberhorn_oncrpc_req_alloc()
 * @in_buf:    raw XDR-encoded payload bytes as received from the kernel,
 * pointing past the ONC RPC call header (i.e. at the procedure arguments)
 * @in_bytes:  length of in_buf in bytes
 *
 * Returns TRUE on success, FALSE if XDR decoding failed (buffer too short,
 * type mismatch, etc.)
 */
int lauberhorn_oncrpc_unmarshal(struct lauberhorn_oncrpc_schema *schema,
                                lauberhorn_msg_t out_msg, uint8_t *in_buf,
                                size_t in_bytes);

// ONC RPC is defined in RFC 5531

// from: https://datatracker.ietf.org/doc/html/rfc5531#section-4

#endif // LAUBERHORN_RT_ONCRPC_H