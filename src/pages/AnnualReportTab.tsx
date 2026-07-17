import { useState, useEffect } from 'react'
import { Loader2, CalendarDays, Zap, MessageCircle, Users, TrendingUp, Clock, Image, Mic, Video } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'

interface AnnualData {
  year: number; totalMessages: number
  totalGroups: number; totalContacts: number
  mostActiveMonth: string; mostActiveDay: string; mostActiveHour: number
  topGroups: [string, number][]; topContacts: [string, number][]
  topPhrases: [string, number][]
  monthlyBreakdown: [string, number][]
  hourlyHeatmap: number[]
  weekdayBreakdown: [string, number][]
  totalTextLength: number; totalImages: number; totalVoices: number; totalVideos: number
  typeBreakdown: Record<string, number>
  longestConversationDay: [string, number]
  earliestMessage: string; latestMessage: string
  activeContactsCount: number; activeGroupsCount: number; newContactsYear: number
}

function AnnualReportTab() {
  const { extractedKey, selectedDb } = useAppStore()
  const [years, setYears] = useState<number[]>([])
  const [year, setYear] = useState(new Date().getFullYear())
  const [loading, setLoading] = useState(false)
  const [generating, setGenerating] = useState(false)
  const [data, setData] = useState<AnnualData | null>(null)
  const [error, setError] = useState('')

  const canUse = !!extractedKey && !!selectedDb

  useEffect(() => {
    if (!canUse) return
    setLoading(true)
    api.getAvailableYears({ db_path: selectedDb!.path, key: extractedKey! })
      .then((ys: number[]) => { setYears(ys); if (ys.length > 0) setYear(ys[ys.length-1]) })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [canUse])

  const generate = async () => {
    setGenerating(true); setError(''); setData(null)
    try {
      const result = await api.generateAnnualReport({ db_path: selectedDb!.path, key: extractedKey!, year })
      setData(result as AnnualData)
    } catch (e: any) { setError(e.message || String(e)) }
    setGenerating(false)
  }

  if (!canUse) return <div className="card"><p className="text-muted">请先提取密钥并选择数据库</p></div>

  const maxHeat = data ? Math.max(...data.hourlyHeatmap, 1) : 1
  const heatColor = (v: number) => {
    const p = v / maxHeat
    if (p === 0) return '#f1f5f9'
    const r = Math.round(79+(180-79)*p), g = Math.round(70+(130-70)*(1-p)), b = Math.round(229+(255-229)*(1-p))
    return `rgb(${r},${g},${b})`
  }

  return (
    <div>
      <div className="card">
        <h2><CalendarDays size={18} style={{marginRight:8,verticalAlign:'middle'}} />年度报告</h2>
        <p className="text-muted" style={{marginBottom:12}}>生成年度消息统计报告，回顾你的 QQ 社交生活</p>
        {loading && <p className="loading-hint"><Loader2 size={14} className="spin" /> 加载可用年份...</p>}
        {!loading && (
          <div style={{display:'flex',gap:8,alignItems:'center'}}>
            <select value={year} onChange={e => setYear(Number(e.target.value))}
              style={{flex:1,padding:'8px 12px',borderRadius:8,border:'1px solid var(--border)',fontSize:13,fontFamily:'inherit'}}>
              {years.map(y => <option key={y} value={y}>{y} 年</option>)}
              {years.length === 0 && <option value="">无可用年份</option>}
            </select>
            <button className="btn-primary" onClick={generate} disabled={years.length===0 || generating}>
              {generating ? <Loader2 size={16} className="spin" /> : <Zap size={16} />}
              {generating ? '分析中...' : '生成报告'}
            </button>
          </div>
        )}
        {error && <p style={{color:'var(--error)',fontSize:13,marginTop:8}}>⚠ {error}</p>}
      </div>

      {data && (
        <>
          {/* Overview */}
          <div className="stats-grid">
            <div className="stat-card"><div className="stat-number">{data.totalMessages.toLocaleString()}</div><div className="stat-label">总消息数</div></div>
            <div className="stat-card"><div className="stat-number">{data.totalContacts}</div><div className="stat-label">联系人</div></div>
            <div className="stat-card"><div className="stat-number">{data.totalGroups}</div><div className="stat-label">群聊</div></div>
            <div className="stat-card"><div className="stat-number">{data.activeDays}</div><div className="stat-label">活跃天数</div></div>
            <div className="stat-card"><div className="stat-number">{data.newContactsYear}</div><div className="stat-label">新增好友</div></div>
            <div className="stat-card"><div className="stat-number">{data.totalTextLength.toLocaleString()}</div><div className="stat-label">总字数</div></div>
          </div>

          {/* Activity peaks */}
          <div className="card">
            <h2><Zap size={16} style={{marginRight:6,verticalAlign:'middle'}} />活跃峰值</h2>
            <div style={{display:'grid',gridTemplateColumns:'1fr 1fr 1fr',gap:16}}>
              <div className="stat-card"><div className="stat-number" style={{fontSize:20}}>{data.mostActiveMonth}</div><div className="stat-label">最活跃月份</div></div>
              <div className="stat-card"><div className="stat-number" style={{fontSize:20}}>{data.mostActiveDay}</div><div className="stat-label">最活跃日</div></div>
              <div className="stat-card"><div className="stat-number">{data.mostActiveHour}:00</div><div className="stat-label">最活跃时段</div></div>
            </div>
          </div>

          {/* Ranks */}
          <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:16}}>
            <div className="card">
              <h2><Users size={16} style={{marginRight:6,verticalAlign:'middle'}} />TOP 群聊</h2>
              <div className="report-table" style={{maxHeight:200,overflowY:'auto'}}>
                <table><thead><tr><th>#</th><th>群名</th><th>消息</th></tr></thead><tbody>
                  {data.topGroups.slice(0,10).map(([n,c],i) => (
                    <tr key={n}><td>{i+1}</td><td>{n}</td><td className="bar-cell"><div className="bar" style={{width:`${data.totalMessages>0?(c/data.totalMessages*100):0}%`}} /><span className="bar-text">{c.toLocaleString()}</span></td></tr>
                  ))}
                </tbody></table>
              </div>
            </div>
            <div className="card">
              <h2><MessageCircle size={16} style={{marginRight:6,verticalAlign:'middle'}} />TOP 好友</h2>
              <div className="report-table" style={{maxHeight:200,overflowY:'auto'}}>
                <table><thead><tr><th>#</th><th>QQ</th><th>消息</th></tr></thead><tbody>
                  {data.topContacts.slice(0,10).map(([n,c],i) => (
                    <tr key={n}><td>{i+1}</td><td>{n}</td><td className="bar-cell"><div className="bar" style={{width:`${data.totalMessages>0?(c/data.totalMessages*100):0}%`}} /><span className="bar-text">{c.toLocaleString()}</span></td></tr>
                  ))}
                </tbody></table>
              </div>
            </div>
          </div>

          {/* Heatmap + Weekday */}
          <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:16}}>
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
            </div>
            <div className="card">
              <h2>周分布</h2>
              <div style={{display:'flex',alignItems:'flex-end',gap:8,height:120}}>
                {data.weekdayBreakdown.map(([d,c]) => (
                  <div key={d} style={{flex:1,textAlign:'center'}}>
                    <div style={{background:'var(--accent)',borderRadius:'4px 4px 0 0',height:`${Math.max(c/Math.max(...data.weekdayBreakdown.map(([_,v])=>v),1)*100,2)}%`,minHeight:4}} />
                    <div style={{fontSize:11,color:'var(--muted)',marginTop:4}}>{d}</div>
                    <div style={{fontSize:10,color:'var(--muted)'}}>{c}</div>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Media breakdown */}
          <div className="card">
            <h2>媒体统计</h2>
            <div className="stats-grid">
              <div className="stat-card"><Image size={20} style={{display:'block',margin:'0 auto 4px',color:'var(--accent)'}} /><div className="stat-number">{data.totalImages.toLocaleString()}</div><div className="stat-label">图片</div></div>
              <div className="stat-card"><Mic size={20} style={{display:'block',margin:'0 auto 4px',color:'var(--accent)'}} /><div className="stat-number">{data.totalVoices.toLocaleString()}</div><div className="stat-label">语音</div></div>
              <div className="stat-card"><Video size={20} style={{display:'block',margin:'0 auto 4px',color:'var(--accent)'}} /><div className="stat-number">{data.totalVideos.toLocaleString()}</div><div className="stat-label">视频</div></div>
              <div className="stat-card"><div className="stat-number">{data.totalTextLength.toLocaleString()}</div><div className="stat-label">总字数</div></div>
            </div>
          </div>

          {/* Monthly trend */}
          <div className="card">
            <h2><TrendingUp size={16} style={{marginRight:6,verticalAlign:'middle'}} />月度趋势</h2>
            <div className="report-table" style={{maxHeight:200,overflowY:'auto'}}>
              <table><thead><tr><th>月份</th><th>消息数</th><th>占比</th></tr></thead><tbody>
                {data.monthlyBreakdown.map(([m,c]) => (
                  <tr key={m}><td>{m}</td><td className="bar-cell"><div className="bar" style={{width:`${data.totalMessages>0?(c/data.totalMessages*100):0}%`}} /><span className="bar-text">{c.toLocaleString()}</span></td><td>{data.totalMessages>0?Math.round(c/data.totalMessages*100):0}%</td></tr>
                ))}
              </tbody></table>
            </div>
          </div>

          {/* Phrases */}
          <div className="card">
            <h2>年度高频词</h2>
            <div>{data.topPhrases.slice(0,30).map(([p,c]) => <span key={p} className="phrase-tag">{p} <small>({c})</small></span>)}</div>
          </div>

          {/* Footer info */}
          <div className="card">
            <div className="flex-between">
              <div><span className="text-muted">最早消息:</span> {data.earliestMessage}</div>
              <div><span className="text-muted">最晚消息:</span> {data.latestMessage}</div>
              <div><span className="text-muted">最长聊天日:</span> {data.longestConversationDay[0]} ({data.longestConversationDay[1]} 条)</div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

export default AnnualReportTab
