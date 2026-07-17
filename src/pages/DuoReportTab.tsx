import { useState, useEffect } from 'react'
import { Loader2, Heart, Clock, MessageCircle, Calendar, Zap, TrendingUp } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'

interface DuoData {
  peerName: string; peerQq: string; selfName: string
  totalMessages: number; myMessages: number; peerMessages: number
  firstMessage: string; lastMessage: string; chatAgeDays: number
  activeDays: number; mostActiveHour: number; mostActiveWeekday: string
  monthlyTrend: [string, number][]
  hourlyHeatmap: number[]
  totalWords: number; avgWordsPerMsg: number
  longestMessage: string
  topPhrases: [string, number][]
  myExclusivePhrases: [string, number][]
  peerExclusivePhrases: [string, number][]
  initiative: [number, number]
  responseAvgHours: number; responseFastestHours: number
  streakDays: number; streakStart: string; streakEnd: string
  typeBreakdown: Record<string, number>
  myImages: number; peerImages: number
  myVoices: number; peerVoices: number
}

function DuoReportTab() {
  const { extractedKey, selectedDb } = useAppStore()
  const [contacts, setContacts] = useState<{id:string;name:string;count:number}[]>([])
  const [selected, setSelected] = useState('')
  const [loading, setLoading] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [data, setData] = useState<DuoData | null>(null)
  const [error, setError] = useState('')

  const canUse = !!extractedKey && !!selectedDb

  useEffect(() => {
    if (!canUse) return
    setLoading(true)
    api.analyzePrivate({ db_path: selectedDb!.path, key: extractedKey! })
      .then(res => {
        if (res.ok && res.data?.groups) {
          const cs = (res.data.groups as any[]).sort((a:any,b:any) => b.messageCount - a.messageCount)
          setContacts(cs.map(c => ({ id: c.id, name: c.name || c.id, count: c.messageCount })))
        }
      }).catch(() => {}).finally(() => setLoading(false))
  }, [canUse])

  const generate = async () => {
    if (!selected) return
    setGenerating(true); setError(''); setData(null)
    try {
      const result = await api.generateDuoReport({
        db_path: selectedDb!.path, key: extractedKey!, peer_uid: selected
      })
      setData(result as DuoData)
    } catch (e: any) { setError(e.message || String(e)) }
    setGenerating(false)
  }

  if (!canUse) return <div className="card"><p className="text-muted">请先提取密钥并选择数据库</p></div>

  const selContact = contacts.find(c => c.id === selected)
  const maxHeat = data ? Math.max(...data.hourlyHeatmap, 1) : 1
  const heatColor = (v: number) => {
    const p = v / maxHeat
    if (p === 0) return '#f1f5f9'
    const r = Math.round(79 + (180-79)*p), g = Math.round(70 + (130-70)*(1-p)), b = Math.round(229 + (255-229)*(1-p))
    return `rgb(${r},${g},${b})`
  }

  return (
    <div>
      {/* Contact selector */}
      <div className="card">
        <h2><Heart size={18} style={{marginRight:8,verticalAlign:'middle'}} />双人报告</h2>
        <p className="text-muted" style={{marginBottom:12}}>选择一个联系人，生成你们之间的深度对话分析报告</p>
        {loading && <p className="loading-hint"><Loader2 size={14} className="spin" /> 加载联系人...</p>}
        {!loading && (
          <div style={{display:'flex',gap:8,alignItems:'center'}}>
            <select value={selected} onChange={e => setSelected(e.target.value)}
              style={{flex:1,padding:'8px 12px',borderRadius:8,border:'1px solid var(--border)',fontSize:13,fontFamily:'inherit'}}>
              <option value="">选择联系人...</option>
              {contacts.map(c => (
                <option key={c.id} value={c.id}>{c.name} ({c.count.toLocaleString()} 条)</option>
              ))}
            </select>
            <button className="btn-primary" onClick={generate} disabled={!selected || generating}>
              {generating ? <Loader2 size={16} className="spin" /> : <Zap size={16} />}
              {generating ? '分析中...' : '生成报告'}
            </button>
          </div>
        )}
        {error && <p style={{color:'var(--error)',fontSize:13,marginTop:8}}>⚠ {error}</p>}
      </div>

      {data && (
        <>
          {/* Overview stats */}
          <div className="stats-grid">
            <div className="stat-card"><div className="stat-number">{data.totalMessages.toLocaleString()}</div><div className="stat-label">总消息</div></div>
            <div className="stat-card"><div className="stat-number">{data.chatAgeDays}</div><div className="stat-label">相识天数</div></div>
            <div className="stat-card"><div className="stat-number">{data.activeDays}</div><div className="stat-label">活跃天数</div></div>
            <div className="stat-card"><div className="stat-number">{data.totalWords.toLocaleString()}</div><div className="stat-label">总字数</div></div>
            <div className="stat-card" style={{background:'#eef2ff'}}><div className="stat-number" style={{fontSize:20}}>{data.peerName}</div><div className="stat-label">{data.peerQq}</div></div>
          </div>

          {/* Message balance */}
          <div className="card">
            <h2><MessageCircle size={16} style={{marginRight:6,verticalAlign:'middle'}} />消息平衡</h2>
            <div style={{display:'flex',gap:20,alignItems:'center'}}>
              <div style={{flex:1,textAlign:'center'}}>
                <div style={{fontSize:24,fontWeight:700,color:'var(--accent)'}}>{data.myMessages.toLocaleString()}</div>
                <div className="text-muted">我发送的</div>
              </div>
              <div style={{flex:2}}>
                <div className="progress-bar"><div className="progress-fill" style={{width:`${data.totalMessages>0?(data.myMessages/data.totalMessages*100):0}%`}} /></div>
                <div style={{display:'flex',justifyContent:'space-between',fontSize:11,color:'var(--muted)',marginTop:4}}>
                  <span>我 {data.totalMessages>0?Math.round(data.myMessages/data.totalMessages*100):0}%</span>
                  <span>{data.peerName} {data.totalMessages>0?Math.round(data.peerMessages/data.totalMessages*100):0}%</span>
                </div>
              </div>
              <div style={{flex:1,textAlign:'center'}}>
                <div style={{fontSize:24,fontWeight:700,color:'var(--text)'}}>{data.peerMessages.toLocaleString()}</div>
                <div className="text-muted">{data.peerName}发送的</div>
              </div>
            </div>
          </div>

          {/* Interaction stats */}
          <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:16}}>
            <div className="card">
              <h2><Zap size={16} style={{marginRight:6,verticalAlign:'middle'}} />互动模式</h2>
              <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:12}}>
                <div className="stat-card"><div className="stat-number">{data.initiative[0]}</div><div className="stat-label">我发起的对话</div></div>
                <div className="stat-card"><div className="stat-number">{data.initiative[1]}</div><div className="stat-label">对方发起的对话</div></div>
                <div className="stat-card"><div className="stat-number">{data.responseAvgHours.toFixed(1)}h</div><div className="stat-label">平均回复间隔</div></div>
                <div className="stat-card"><div className="stat-number">{data.responseFastestHours.toFixed(1)}h</div><div className="stat-label">最快回复</div></div>
              </div>
              {data.streakDays > 0 && (
                <div style={{marginTop:12,padding:12,background:'#fef3c7',borderRadius:8,fontSize:13}}>
                  🔥 最长连续聊天 <strong>{data.streakDays}</strong> 天 ({data.streakStart} ~ {data.streakEnd})
                </div>
              )}
            </div>

            <div className="card">
              <h2><Clock size={16} style={{marginRight:6,verticalAlign:'middle'}} />24小时活跃分布</h2>
              <div className="heatmap">
                {data.hourlyHeatmap.map((v, i) => (
                  <div key={i} className="heatmap-col">
                    <div className="heatmap-cell" style={{background:heatColor(v)}} title={`${i}:00 — ${v} 条`} />
                    <div className="heatmap-label">{i}h</div>
                  </div>
                ))}
              </div>
              <div className="text-muted" style={{marginTop:8}}>最活跃: {data.mostActiveHour}:00 ({data.hourlyHeatmap[data.mostActiveHour]} 条)</div>
            </div>
          </div>

          {/* Media stats */}
          <div className="card">
            <h2>媒体统计</h2>
            <div className="stats-grid">
              <div className="stat-card"><div className="stat-number">{data.myImages}</div><div className="stat-label">我发的图片</div></div>
              <div className="stat-card"><div className="stat-number">{data.peerImages}</div><div className="stat-label">对方发的图片</div></div>
              <div className="stat-card"><div className="stat-number">{data.myVoices}</div><div className="stat-label">我发的语音</div></div>
              <div className="stat-card"><div className="stat-number">{data.peerVoices}</div><div className="stat-label">对方发的语音</div></div>
            </div>
          </div>

          {/* Monthly trend */}
          <div className="card">
            <h2><TrendingUp size={16} style={{marginRight:6,verticalAlign:'middle'}} />月度趋势</h2>
            <div className="report-table" style={{maxHeight:200,overflowY:'auto'}}>
              <table><thead><tr><th>月份</th><th>消息数</th><th>占比</th></tr></thead><tbody>
                {data.monthlyTrend.map(([m, c]) => (
                  <tr key={m}><td>{m}</td><td className="bar-cell"><div className="bar" style={{width:`${data.totalMessages>0?(c/data.totalMessages*100):0}%`}} /><span className="bar-text">{c.toLocaleString()}</span></td><td>{data.totalMessages>0?Math.round(c/data.totalMessages*100):0}%</td></tr>
                ))}
              </tbody></table>
            </div>
          </div>

          {/* Phrases */}
          <div className="card">
            <h2>高频词汇</h2>
            <div style={{marginBottom:12}}>{data.topPhrases.slice(0,20).map(([p,c]) => <span key={p} className="phrase-tag">{p} <small>({c})</small></span>)}</div>
            {data.myExclusivePhrases.length > 0 && <><h3 className="text-muted" style={{fontSize:12,marginBottom:6}}>我的专属词</h3><div style={{marginBottom:12}}>{data.myExclusivePhrases.slice(0,10).map(([p,c]) => <span key={p} className="phrase-tag">{p} <small>({c})</small></span>)}</div></>}
            {data.peerExclusivePhrases.length > 0 && <><h3 className="text-muted" style={{fontSize:12,marginBottom:6}}>{data.peerName}的专属词</h3><div>{data.peerExclusivePhrases.slice(0,10).map(([p,c]) => <span key={p} className="phrase-tag">{p} <small>({c})</small></span>)}</div></>}
          </div>

          {/* Info */}
          <div className="card">
            <div className="flex-between">
              <div><span className="text-muted">首条消息:</span> {data.firstMessage}</div>
              <div><span className="text-muted">末条消息:</span> {data.lastMessage}</div>
              <div><span className="text-muted">相识:</span> {data.chatAgeDays} 天</div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

export default DuoReportTab
