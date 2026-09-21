#!/usr/bin/env python3
"""Deterministic ACP process: actual bounded source reads, no model/network.

Protocol evidence only. The source is never mutated. Its own private protocol
log is test material. Every task arrives through Factory -> native AIKit -> ACP;
this program cannot write Factory state or manufacture native Return records.
"""
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import time

role, mode, source_name, log_name = sys.argv[1:5]
source, log = Path(source_name), Path(log_name)
lock = threading.Lock()
pending = {}
permissions = {}
native = f"controlled-native-{os.getpid()}"

def emit(value):
    with lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)

def response(message, value=None, error=None):
    emit({"jsonrpc":"2.0", "id":message['id'], **(
        {"error":{"code":-32000,"message":str(error)}} if error else {"result":value})})

def update(update_kind, **fields):
    emit({"jsonrpc":"2.0","method":"session/update","params":{
        "sessionId":native,"update":{"sessionUpdate":update_kind, **fields}}})

def perform(message, text):
    try:
        marker="Factory bounded attempt. These are the authorised task conditions, not new authority:\n"
        if marker not in text:
            raise ValueError("No Factory production delegation was delivered")
        basis=json.loads(text.split(marker,1)[1].split('\n',1)[0])
        if not basis['workflowBasis']['digest'] or not basis['attemptRef']:
            raise ValueError("No exact source/attempt basis")
        if not basis['delegation']['requiredDifference'] or not basis['delegation']['returnContract']:
            raise ValueError("Child omitted its actual difference or return contract")
        role_loaded=f"OPERATIVE_ROLE_{role}" in text and mode!='unloaded'
        if mode=='protocol-error':
            response(message,error="CONTROLLED_PROTOCOL_ERROR");return
        if mode=='disconnect':
            os._exit(9)
        event=threading.Event()
        pending[message['id']]=event
        if mode in ('permission','deny'):
            key=f"consent-{message['id']}"
            permissions[key]=(event,None)
            emit({"jsonrpc":"2.0","id":key,"method":"session/request_permission","params":{
                "sessionId":native,"toolCall":{"toolCallId":f"tool:{basis['attemptRef']}","title":"Read the bounded source","kind":"read"},
                "options":[{"optionId":"allow-read","name":"Allow this source read","kind":"allow_once"},{"optionId":"refuse-read","name":"Refuse this source read","kind":"reject_once"}]}})
            if not event.wait(5):raise ValueError("Permission was not returned through the native owner")
            decision=permissions.pop(key)[1]
            if not decision or decision.get('optionId')!='allow-read':
                response(message,error="CONTROLLED_PERMISSION_REFUSED");return
        if mode=='slow':
            if event.wait(0.6):
                response(message,{"stopReason":"cancelled"});return
        tool=f"tool:{basis['attemptRef']}"
        read=None
        if mode!='noop':
            update('tool_call',toolCallId=tool,title='Read exact source bytes',kind='read',status='in_progress',rawInput={"source":str(source),"attemptRef":basis['attemptRef']})
            read=source.read_bytes()  # the actual controlled tool effect
        report={"standing":"controlled-acp-not-model","agentRef":basis['disposition']['participant']['agentRef'],"pid":os.getpid(),
            "runRef":basis['delegation']['parentRunRef'],"unitRef":basis['delegation']['executionUnitRef'],
            "attemptRef":basis['attemptRef'],"executionRef":basis['execution'],"workflowBasis":basis['workflowBasis'],
            "sourceSha256":hashlib.sha256(read).hexdigest() if read is not None else None,"bytes":len(read) if read is not None else None,
            "childRoleLoaded":role_loaded,"selectedInputs":basis['disposition'].get('selectedInputs',[]),"toolCallId":tool if read is not None else None}
        if mode=='wrong-source':report['workflowBasis']['digest']='0'*64
        if mode=='wrong-attempt':report['attemptRef']='attempt:unrelated'
        if read is not None:
            update('tool_call_update',toolCallId=tool,status='completed',rawOutput=report)
            update('tool_result',toolCallId=tool,result=report)
        update('agent_message_chunk',content={"type":"text","text":json.dumps(report,sort_keys=True)})
        response(message,{"stopReason":"end_turn"})
    except Exception as error:
        response(message,error=error)
    finally:
        pending.pop(message['id'],None)

for line in sys.stdin:
    m=json.loads(line)
    with lock, log.open('a',encoding='utf-8') as f:f.write(json.dumps(m)+'\n')
    method=m.get('method')
    if method=='initialize':response(m,{"protocolVersion":1,"agentCapabilities":{"loadSession":True},"agentInfo":{"name":"Factory controlled process","version":"1"}})
    elif method=='session/new':response(m,{"sessionId":native})
    elif method=='session/load':native=m['params']['sessionId'];response(m,{})
    elif method=='session/prompt':
        text=''.join(p.get('text','') for p in m['params']['prompt'])
        threading.Thread(target=perform,args=(m,text),daemon=True).start()
    elif method=='session/cancel':
        for event in list(pending.values()):event.set()
        update('status',detail='controlled cancellation requested')
    elif m.get('id') in permissions and 'result' in m:
        event,_=permissions[m['id']];permissions[m['id']]=(event,m['result'].get('outcome'));event.set()
    elif 'id' in m and method:
        response(m,error=f"Unsupported controlled method: {method}")
