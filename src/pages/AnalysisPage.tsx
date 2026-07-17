import { useState } from 'react'
import { BarChart3, Users, MessageCircle, Heart, CalendarDays } from 'lucide-react'
import GroupAnalysisTab from './GroupAnalysisTab'
import PrivateAnalysisTab from './PrivateAnalysisTab'
import DuoReportTab from './DuoReportTab'
import AnnualReportTab from './AnnualReportTab'
import './AnalysisPage.scss'

type Tab = 'group' | 'private' | 'duo' | 'annual'

function AnalysisPage() {
  const [tab, setTab] = useState<Tab>('group')

  return (
    <div className="main-content">
      <div className="page-header">
        <BarChart3 size={24} className="page-header-icon" />
        <h1>数据分析</h1>
      </div>
      <div className="tab-bar">
        <button className={`tab-btn ${tab === 'group' ? 'active' : ''}`} onClick={() => setTab('group')}>
          <Users size={16} /> 群聊分析
        </button>
        <button className={`tab-btn ${tab === 'private' ? 'active' : ''}`} onClick={() => setTab('private')}>
          <MessageCircle size={16} /> 私聊分析
        </button>
        <button className={`tab-btn ${tab === 'duo' ? 'active' : ''}`} onClick={() => setTab('duo')}>
          <Heart size={16} /> 双人报告
        </button>
        <button className={`tab-btn ${tab === 'annual' ? 'active' : ''}`} onClick={() => setTab('annual')}>
          <CalendarDays size={16} /> 年度报告
        </button>
      </div>
      <div className="tab-content">
        {tab === 'group' && <GroupAnalysisTab />}
        {tab === 'private' && <PrivateAnalysisTab />}
        {tab === 'duo' && <DuoReportTab />}
        {tab === 'annual' && <AnnualReportTab />}
      </div>
    </div>
  )
}

export default AnalysisPage
